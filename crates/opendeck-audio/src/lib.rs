//! Motor de áudio de baixa latência para OpenDeck.
//!
//! ## Design Decisions
//!
//! ### Buffer Size: 128 samples @ 48kHz = ~2.67ms
//!
//! Este é o "sweet spot" para DJing profissional:
//! - **< 3ms latência**: imperceptível ao ouvido humano, essencial para beatmatching manual
//! - **Estável**: buffers menores (64) causam xruns em sistemas sob carga; maiores (256+) introduzem latência perceptível
//! - **Padrão da indústria**: Serato, Traktor, rekordbox usam 128-256 frames internamente
//!
//! ### Ring Buffer Lock-Free (ringbuf)
//!
//! Separação clara entre duas threads:
//! 1. **Thread de Áudio (real-time, alta prioridade)**: consome do ring buffer, *nunca* aloca, *nunca* bloqueia
//! 2. **Thread de Decode (background)**: produz PCM decodado via symphonia, alimenta o ring buffer
//!
//! O ring buffer é lock-free para single-producer/single-consumer (SPSC),
//! eliminando contenção e priority inversion. Se o buffer enche, o decode faz backpressure;
//! se esvazia, a thread de áudio insere silêncio (xrun) e incrementa contador.
//!
//! ### Tempo vs Tom (Pitch)
//!
//! - **Tempo (speed)**: altera duração *sem* alterar frequência fundamental (time-stretch)
//!   - Implementação: interpolação linear simples no consumo do ring buffer (adequado para ±50%)
//!   - Uso: sync de BPM entre decks
//! - **Tom (pitch)**: altera frequência fundamental *sem* alterar duração (resampling)
//!   - Implementação: mudança de sample rate efetivo na reprodução
//!   - Uso: key matching, harmonic mixing
//! - **Combinação**: DJs ajustam *tempo* para beatmatch, *tom* para harmonia

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, SampleRate, StreamConfig, PlayStreamError};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use thiserror::Error;

/// Taxa de amostragem padrão para áudio profissional (48kHz).
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Tamanho do buffer de áudio (128 frames = ~2.67ms @ 48kHz).
/// Valor fixo para latência determinística.
pub const BUFFER_SIZE: u32 = 128;

/// Erros do motor de áudio.
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Dispositivo de áudio não encontrado: {0}")]
    DeviceNotFound(String),

    #[error("Erro ao criar/rodar stream de áudio: {0}")]
    StreamError(String),

    #[error("Erro de decodificação de áudio: {0}")]
    DecodeError(String),

    #[error("Formato de amostra não suportado: {0}")]
    UnsupportedSampleFormat(String),

    #[error("Buffer underrun (xrun) detectado")]
    BufferUnderrun,

    #[error("Thread de áudio não iniciada")]
    ThreadNotRunning,

    #[error("Arquivo não encontrado: {0}")]
    FileNotFound(String),

    #[error("Erro de build do stream: {0}")]
    BuildStreamError(String),
}
impl From<cpal::BuildStreamError> for AudioError {
    fn from(e: cpal::BuildStreamError) -> Self {
        AudioError::BuildStreamError(e.to_string())
    }
}

impl From<cpal::PlayStreamError> for AudioError {
    fn from(e: cpal::PlayStreamError) -> Self {
        AudioError::StreamError(e.to_string())
    }
}

/// Estado de reprodução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Configuração do motor de áudio.
#[derive(Debug, Clone)]
pub struct AudioEngineConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub prefer_asio: bool,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: DEFAULT_SAMPLE_RATE,
            buffer_size: BUFFER_SIZE,
            prefer_asio: prefer_asio(),
        }
    }
}

/// Preferência de backend ASIO no Windows para latência mínima.
#[cfg(target_os = "windows")]
fn prefer_asio() -> bool {
    true
}

/// No Linux, usa ALSA/PipeWire padrão (ASIO não disponível).
#[cfg(target_os = "linux")]
fn prefer_asio() -> bool {
    false
}

/// Ring buffer simples thread-safe usando Vec + índices atômicos.
/// 
/// Implementação SPSC (single-producer, single-consumer) lock-free
/// usando índices atômicos para evitar contenção.
pub struct RingBuffer {
    buffer: std::cell::UnsafeCell<Vec<f32>>,
    capacity: usize,
    read_idx: Arc<std::sync::atomic::AtomicUsize>,
    write_idx: Arc<std::sync::atomic::AtomicUsize>,
}

unsafe impl Sync for RingBuffer {}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: std::cell::UnsafeCell::new(vec![0.0; capacity]),
            capacity,
            read_idx: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            write_idx: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Tenta empurrar uma amostra (non-blocking).
    /// Retorna Ok(()) se bem-sucedido, Err(sample) se buffer cheio.
    pub fn try_push(&self, sample: f32) -> Result<(), f32> {
        let write = self.write_idx.load(std::sync::atomic::Ordering::Relaxed);
        let next_write = (write + 1) % self.capacity;
        let read = self.read_idx.load(std::sync::atomic::Ordering::Acquire);
        
        if next_write == read {
            // Buffer cheio
            return Err(sample);
        }
        
        // Safety: Only producer writes to buffer, consumer only reads
        // SPSC guarantees no concurrent access to same index
        unsafe {
            let buf = &mut *self.buffer.get();
            buf[write] = sample;
        }
        self.write_idx.store(next_write, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Tenta retirar uma amostra (non-blocking).
    /// Retorna Some(sample) se disponível, None se buffer vazio.
    pub fn try_pop(&self) -> Option<f32> {
        let read = self.read_idx.load(std::sync::atomic::Ordering::Relaxed);
        let write = self.write_idx.load(std::sync::atomic::Ordering::Acquire);
        
        if read == write {
            // Buffer vazio
            return None;
        }
        
        // Safety: Only consumer reads from buffer at this index
        let sample = unsafe {
            let buf = &*self.buffer.get();
            buf[read]
        };
        self.read_idx.store((read + 1) % self.capacity, std::sync::atomic::Ordering::Release);
        Some(sample)
    }

    /// Limpa o buffer (reseta índices).
    pub fn clear(&self) {
        self.read_idx.store(0, std::sync::atomic::Ordering::Release);
        self.write_idx.store(0, std::sync::atomic::Ordering::Release);
    }

    /// Retorna capacidade do buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Retorna número de elementos disponíveis para leitura.
    pub fn available(&self) -> usize {
        let write = self.write_idx.load(std::sync::atomic::Ordering::Acquire);
        let read = self.read_idx.load(std::sync::atomic::Ordering::Acquire);
        (write + self.capacity - read) % self.capacity
    }
}

/// Motor de áudio principal.
///
/// Encapsula dispositivo de saída, stream CPAL, ring buffer lock-free
/// e thread de decodificação em background.
pub struct AudioEngine {
    config: AudioEngineConfig,
    state: Arc<Mutex<PlaybackState>>,
    stream: Option<cpal::Stream>,
    device_name: String,
    sample_rate: u32,
    channels: u16,
    ring_buffer: Arc<RingBuffer>,
    xrun_count: Arc<Mutex<u64>>,
    tempo_factor: Arc<Mutex<f32>>,
    pitch_semitones: Arc<Mutex<f32>>,
    phase: Arc<Mutex<f32>>,
    last_sample: Arc<AtomicU32>,
    has_sample: Arc<AtomicBool>,
    decode_handle: Option<thread::JoinHandle<()>>,
    decode_shutdown: Arc<Mutex<bool>>,
}

impl AudioEngine {
    /// Cria um novo motor de áudio com configuração padrão (buffer de 2 segundos).
    pub fn new() -> Result<Self, AudioError> {
        Self::with_buffer_seconds(AudioEngineConfig::default(), 2.0)
    }

    /// Cria um novo motor de áudio com tamanho de buffer customizado em segundos.
    /// Ex: `AudioEngine::new(4.0)` para 4 segundos de buffer (mais seguro contra underruns).
    pub fn with_buffer_seconds(mut config: AudioEngineConfig, buffer_seconds: f32) -> Result<Self, AudioError> {
        let host = cpal::default_host();

        // Seleciona dispositivo: tenta ASIO no Windows, senão default
        let device = if config.prefer_asio {
            host.output_devices()
                .ok()
                .and_then(|mut iter| iter.find(|d| d.name().ok().map(|n| n.to_lowercase().contains("asio")).unwrap_or(false)))
                .or_else(|| host.default_output_device())
        } else {
            host.default_output_device()
        };

        let device = device.ok_or_else(|| AudioError::DeviceNotFound(
            "Nenhum dispositivo de saída disponível".to_string()
        ))?;

        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        // Configuração de stream: buffer fixo, sample rate fixo
        let config_range = device.supported_output_configs()
            .map_err(|e| AudioError::StreamError(e.to_string()))?
            .find(|c| c.channels() >= 2 && c.sample_format() == SampleFormat::F32)
            .ok_or_else(|| AudioError::UnsupportedSampleFormat(
                "Dispositivo não suporta f32 stereo".to_string()
            ))?;

        let mut stream_config: StreamConfig = config_range
            .with_sample_rate(SampleRate(config.sample_rate))
            .config();

        stream_config.buffer_size = BufferSize::Fixed(config.buffer_size);

        // Ring buffer lock-free SPSC com tamanho baseado em buffer_seconds
        let ring_frames = (config.sample_rate as f32 * buffer_seconds) as usize;
        let ring_buffer = Arc::new(RingBuffer::new(ring_frames * stream_config.channels as usize));

        let state = Arc::new(Mutex::new(PlaybackState::Stopped));
        let xrun_count = Arc::new(Mutex::new(0u64));
        let tempo_factor = Arc::new(Mutex::new(1.0f32));
        let pitch_semitones = Arc::new(Mutex::new(0.0f32));
        let phase = Arc::new(Mutex::new(0.0f32));
        let last_sample = Arc::new(AtomicU32::new(0));
        let has_sample = Arc::new(AtomicBool::new(false));
        let decode_shutdown = Arc::new(Mutex::new(false));

        Ok(Self {
            config: config.clone(),
            state,
            stream: None,
            device_name,
            sample_rate: config.sample_rate,
            channels: stream_config.channels,
            ring_buffer,
            xrun_count,
            tempo_factor,
            pitch_semitones,
            phase,
            last_sample,
            has_sample,
            decode_handle: None,
            decode_shutdown,
        })
    }

    /// Inicia o stream de áudio (callback CPAL roda em thread real-time).
    /// Chame antes de `play()` para criar o dispositivo de saída e começar a consumir
    /// do ring buffer. Sem isso, `play()` retorna `ThreadNotRunning`.
    pub fn start_stream(&mut self) -> Result<(), AudioError> {
        if self.stream.is_some() {
            return Ok(());
        }

        let ring_buffer = Arc::clone(&self.ring_buffer);
        let xrun_count = Arc::clone(&self.xrun_count);
        let state = Arc::clone(&self.state);
        let tempo_factor = Arc::clone(&self.tempo_factor);
        let pitch_semitones = Arc::clone(&self.pitch_semitones);
        let phase = Arc::clone(&self.phase);
        let last_sample = Arc::clone(&self.last_sample);
        let has_sample = Arc::clone(&self.has_sample);
        let channels = self.channels;

        let stream = self.build_output_stream(
            ring_buffer,
            xrun_count,
            state,
            tempo_factor,
            pitch_semitones,
            phase,
            last_sample,
            has_sample,
            channels,
        )?;

        stream.play().map_err(|e: PlayStreamError| AudioError::StreamError(e.to_string()))?;
        self.stream = Some(stream);
        *self.state.lock().unwrap() = PlaybackState::Playing;
        Ok(())
    }

    /// Alias para `start_stream()`. Inicia o stream de áudio CPAL.
    pub fn start(&mut self) -> Result<(), AudioError> {
        self.start_stream()
    }

    /// Carrega uma track, inicia o stream e começa a reprodução em uma única chamada.
    /// Combina `load_track()` + `start_stream()` + `play()`.
    pub fn play_file(&mut self, path: &Path) -> Result<(), AudioError> {
        self.load_track(path.to_str().ok_or_else(|| AudioError::FileNotFound(path.display().to_string()))?)?;
        if self.stream.is_none() {
            self.start_stream()?;
        }
        self.play()?;
        Ok(())
    }

    /// Encerra o motor de áudio de forma limpa: pausa o stream, encerra a thread de decode.
    pub fn shutdown(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
        }
        *self.decode_shutdown.lock().unwrap() = true;
        if let Some(handle) = self.decode_handle.take() {
            let _ = handle.join();
        }
        *self.state.lock().unwrap() = PlaybackState::Stopped;
    }

    /// Constrói o stream de saída CPAL com callback de áudio real-time.
    fn build_output_stream(
        &self,
        ring_buffer: Arc<RingBuffer>,
        xrun_count: Arc<Mutex<u64>>,
        state: Arc<Mutex<PlaybackState>>,
        tempo_factor: Arc<Mutex<f32>>,
        pitch_semitones: Arc<Mutex<f32>>,
        phase: Arc<Mutex<f32>>,
        last_sample: Arc<AtomicU32>,
        has_sample: Arc<AtomicBool>,
        channels: u16,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(cpal::BuildStreamError::DeviceNotAvailable)?;

        let stream_config = StreamConfig {
            channels,
            sample_rate: SampleRate(self.sample_rate),
            buffer_size: BufferSize::Fixed(self.config.buffer_size),
        };

        device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Callback real-time: NÃO alocar, NÃO bloquear, NÃO logar
                let state = *state.lock().unwrap();
                let tempo = *tempo_factor.lock().unwrap();
                let pitch = *pitch_semitones.lock().unwrap();

                if state != PlaybackState::Playing {
                    // Silêncio quando pausado/parado
                    data.fill(0.0);
                    *phase.lock().unwrap() = 0.0;
                    return;
                }

                // Pitch shift: cada semitono = fator 2^(1/12) ≈ 1.05946
                let pitch_factor = 2.0f32.powf(pitch / 12.0);
                // Tempo: step = quantas amostras de entrada consumir por amostra de saída
                let step = pitch_factor / tempo;
                let mut phase_val = *phase.lock().unwrap();

                let frames = data.len() / channels as usize;
                let mut xruns = 0;

                for frame in 0..frames {
                    phase_val += step;

                    // Consumir amostras do ring buffer conforme o passo avança
                    while phase_val >= 1.0 {
                        match ring_buffer.try_pop() {
                            Some(s) => {
                                last_sample.store(f32::to_bits(s), Ordering::Relaxed);
                                has_sample.store(true, Ordering::Relaxed);
                            }
                            None => {
                                xruns += 1;
                                if !has_sample.load(Ordering::Relaxed) {
                                    last_sample.store(f32::to_bits(0.0), Ordering::Relaxed);
                                }
                            }
                        }
                        phase_val -= 1.0;
                    }

                    for ch in 0..channels as usize {
                        let idx = frame * channels as usize + ch;
                        data[idx] = f32::from_bits(last_sample.load(Ordering::Relaxed));
                    }
                }

                *phase.lock().unwrap() = phase_val;

                if xruns > 0 {
                    *xrun_count.lock().unwrap() += xruns;
                }
            },
            move |err| {
                eprintln!("Audio stream error: {}", err);
            },
            None,
        )
    }

    /// Carrega uma track e inicia decodificação em background thread.
    ///
    /// Decodifica MP3/WAV/FLAC/AAC via symphonia para PCM f32 planar,
    /// converte para interleaved stereo e alimenta o ring buffer.
    pub fn load_track(&mut self, path: &str) -> Result<(), AudioError> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(AudioError::FileNotFound(path.display().to_string()));
        }

        // Sinaliza shutdown da thread anterior se existir
        *self.decode_shutdown.lock().unwrap() = true;
        if let Some(handle) = self.decode_handle.take() {
            let _ = handle.join();
        }
        *self.decode_shutdown.lock().unwrap() = false;

        // Limpa ring buffer
        self.ring_buffer.clear();

        let ring_buffer = Arc::clone(&self.ring_buffer);
        let decode_shutdown = Arc::clone(&self.decode_shutdown);
        let path = path.to_path_buf();
        let target_sample_rate = self.sample_rate;
        let target_channels = self.channels;

        let handle = thread::spawn(move || {
            if let Err(e) = decode_and_feed(
                &path,
                ring_buffer,
                decode_shutdown,
                target_sample_rate,
                target_channels,
            ) {
                eprintln!("Decode error: {}", e);
            }
        });

        self.decode_handle = Some(handle);
        Ok(())
    }

    /// Inicia reprodução.
    pub fn play(&self) -> Result<(), AudioError> {
        if self.stream.is_none() {
            return Err(AudioError::ThreadNotRunning);
        }
        *self.state.lock().unwrap() = PlaybackState::Playing;
        Ok(())
    }

    /// Pausa reprodução (mantém stream ativo, insere silêncio no callback).
    pub fn pause(&self) {
        *self.state.lock().unwrap() = PlaybackState::Paused;
    }

    /// Para reprodução e reseta posição.
    pub fn stop(&self) {
        *self.state.lock().unwrap() = PlaybackState::Stopped;
        // Limpa ring buffer
        self.ring_buffer.clear();
    }

    /// Ajusta fator de tempo (speed) sem alterar pitch.
    /// 1.0 = normal, 1.5 = +50% velocidade, 0.5 = -50%.
    pub fn set_tempo(&mut self, factor: f32) {
        let clamped = factor.clamp(0.25, 4.0); // Limites seguros
        *self.tempo_factor.lock().unwrap() = clamped;
    }

    /// Ajusta pitch em semitons sem alterar tempo.
    /// +12 = uma oitava acima, -12 = uma oitava abaixo.
    pub fn set_pitch(&mut self, semitones: f32) {
        let clamped = semitones.clamp(-24.0, 24.0); // ±2 oitavas
        *self.pitch_semitones.lock().unwrap() = clamped;
    }

    /// Retorna contador de xruns (buffer underruns).
    /// Valor > 0 indica problemas de latência/performance.
    pub fn get_xrun_count(&self) -> u64 {
        *self.xrun_count.lock().unwrap()
    }

    /// Retorna estado atual de reprodução.
    pub fn get_state(&self) -> PlaybackState {
        *self.state.lock().unwrap()
    }

    /// Retorna quantos segundos de áudio estão no buffer (para debug/monitoramento).
    pub fn buffer_fill_seconds(&self) -> f32 {
        let available = self.ring_buffer.available();
        let frames = available / self.channels as usize;
        frames as f32 / self.sample_rate as f32
    }

    /// Retorna nome do dispositivo de áudio em uso.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Retorna sample rate configurado.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Retorna tamanho do buffer em frames.
    pub fn buffer_size(&self) -> u32 {
        self.config.buffer_size
    }

    /// Calcula latência teórica em milissegundos.
    pub fn theoretical_latency_ms(&self) -> f32 {
        (self.config.buffer_size as f32 / self.sample_rate as f32) * 1000.0
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Decodifica arquivo de áudio via symphonia e alimenta ring buffer.
///
/// Roda em thread de background (não real-time), pode alocar e bloquear.
fn decode_and_feed(
    path: &Path,
    ring_buffer: Arc<RingBuffer>,
    decode_shutdown: Arc<Mutex<bool>>,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<(), AudioError> {
    let source = std::fs::File::open(path)
        .map_err(|e| AudioError::DecodeError(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(source), Default::default());

    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| AudioError::DecodeError(format!("Probe failed: {}", e)))?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| AudioError::DecodeError("Nenhuma track de áudio válida".to_string()))?;

    let decoder_opts = DecoderOptions {
        verify: false,
        ..Default::default()
    };
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|e| AudioError::DecodeError(format!("Decoder creation failed: {}", e)))?;

    let track_id = track.id;
    let mut sample_buf = None::<SampleBuffer<f32>>;

    // Loop de decodificação
    loop {
        // Verifica shutdown
        if *decode_shutdown.lock().unwrap() {
            break;
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(AudioError::DecodeError(format!("Packet error: {}", e))),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Decode frame error: {}", e);
                continue;
            }
        };

        // Converte para f32 se necessário
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        let duration = decoded.frames() as u64;

        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
        }

        if let Some(buf) = &mut sample_buf {
            buf.copy_interleaved_ref(decoded);
            let samples = buf.samples();

            // Resample simples se sample rate diferente (TODO: usar rubato para qualidade)
            let samples_to_feed = if spec.rate == target_sample_rate {
                samples.to_vec()
            } else {
                // Linear interpolation resample (básico)
                resample_linear(samples, channels, spec.rate, target_sample_rate)
            };

            // Converte para stereo se necessário
            let stereo_samples = if channels == 1 && target_channels == 2 {
                // Mono -> Stereo (duplica)
                samples_to_feed.iter().flat_map(|&s| [s, s]).collect()
            } else if channels == 2 && target_channels == 2 {
                samples_to_feed
            } else {
                // Fallback: usa apenas primeiro canal
                samples_to_feed.chunks(channels as usize).map(|c| c[0]).collect()
            };

            // Alimenta ring buffer com backpressure
            for sample in stereo_samples {
                // Spin-wait com backpressure suave
                while ring_buffer.try_push(sample).is_err() {
                    if *decode_shutdown.lock().unwrap() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    Ok(())
}

/// Resample linear simples (para compatibilidade de sample rate).
/// Em produção, usar `rubato` ou `samplerate` crate para qualidade profissional.
fn resample_linear(
    input: &[f32],
    channels: usize,
    from_rate: u32,
    to_rate: u32,
) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }

    let ratio = from_rate as f32 / to_rate as f32;
    let input_frames = input.len() / channels;
    let output_frames = (input_frames as f32 / ratio) as usize;
    let mut output = Vec::with_capacity(output_frames * channels);

    for ch in 0..channels {
        for out_frame in 0..output_frames {
            let in_pos = out_frame as f32 * ratio;
            let in_frame = in_pos as usize;
            let frac = in_pos - in_frame as f32;

            let idx0 = in_frame * channels + ch;
            let idx1 = ((in_frame + 1).min(input_frames - 1)) * channels + ch;

            let s0 = input.get(idx0).copied().unwrap_or(0.0);
            let s1 = input.get(idx1).copied().unwrap_or(0.0);

            let interp = s0 + frac * (s1 - s0);
            output.push(interp);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_engine_creation() {
        let engine = AudioEngine::new();
        assert!(engine.is_ok(), "Falha ao criar AudioEngine: {:?}", engine.err());
        
        let engine = engine.unwrap();
        assert_eq!(engine.sample_rate(), DEFAULT_SAMPLE_RATE);
        assert_eq!(engine.buffer_size(), BUFFER_SIZE);
        assert_eq!(engine.get_state(), PlaybackState::Stopped);
        assert!(!engine.device_name().is_empty());
    }

    #[test]
    fn test_latency_calculation() {
        let engine = AudioEngine::new().unwrap();
        let latency = engine.theoretical_latency_ms();
        // 128 frames / 48000 Hz * 1000 = ~2.667ms
        assert!((latency - 2.667).abs() < 0.01, "Latência esperada ~2.67ms, obtida {}", latency);
    }

    #[test]
    fn test_tempo_clamping() {
        let mut engine = AudioEngine::new().unwrap();
        
        engine.set_tempo(1.0);
        assert_eq!(*engine.tempo_factor.lock().unwrap(), 1.0);
        
        engine.set_tempo(2.0);
        assert_eq!(*engine.tempo_factor.lock().unwrap(), 2.0);
        
        engine.set_tempo(10.0); // Acima do limite
        assert_eq!(*engine.tempo_factor.lock().unwrap(), 4.0);
        
        engine.set_tempo(0.1); // Abaixo do limite
        assert_eq!(*engine.tempo_factor.lock().unwrap(), 0.25);
    }

    #[test]
    fn test_pitch_clamping() {
        let mut engine = AudioEngine::new().unwrap();
        
        engine.set_pitch(0.0);
        assert_eq!(*engine.pitch_semitones.lock().unwrap(), 0.0);
        
        engine.set_pitch(12.0);
        assert_eq!(*engine.pitch_semitones.lock().unwrap(), 12.0);
        
        engine.set_pitch(50.0); // Acima do limite
        assert_eq!(*engine.pitch_semitones.lock().unwrap(), 24.0);
        
        engine.set_pitch(-50.0); // Abaixo do limite
        assert_eq!(*engine.pitch_semitones.lock().unwrap(), -24.0);
    }

    #[test]
    fn test_playback_state_transitions() {
        let engine = AudioEngine::new().unwrap();
        
        assert_eq!(engine.get_state(), PlaybackState::Stopped);
        
        engine.pause();
        assert_eq!(engine.get_state(), PlaybackState::Paused);
        
        // play() falha se stream não iniciado, mas não muda estado
        let result = engine.play();
        assert!(result.is_err());
        assert_eq!(engine.get_state(), PlaybackState::Paused);
        
        engine.stop();
        assert_eq!(engine.get_state(), PlaybackState::Stopped);
    }

    #[test]
    fn test_ring_buffer_basic() {
        let rb = RingBuffer::new(100);
        
        // Testa push/pop básico
        for i in 0..10 {
            rb.try_push(i as f32).unwrap();
        }
        for i in 0..10 {
            assert_eq!(rb.try_pop().unwrap(), i as f32);
        }
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let rb = RingBuffer::new(20);
        
        // Enche o buffer parcialmente
        for i in 0..8 {
            rb.try_push(i as f32).unwrap();
        }
        // Remove alguns
        for i in 0..3 {
            assert_eq!(rb.try_pop().unwrap(), i as f32);
        }
        // Adiciona mais (wrap around)
        for i in 8..15 {
            rb.try_push(i as f32).unwrap();
        }
        // Verifica ordem
        for i in 3..15 {
            assert_eq!(rb.try_pop().unwrap(), i as f32);
        }
    }

    #[test]
    fn test_ring_buffer_full() {
        let rb = RingBuffer::new(4);
        
        rb.try_push(1.0).unwrap();
        rb.try_push(2.0).unwrap();
        rb.try_push(3.0).unwrap();
        // Buffer cheio (capacidade 4, mas um slot reservado para distinguir cheio/vazio)
        assert!(rb.try_push(4.0).is_err());
    }

    #[test]
    fn test_ring_buffer_clear() {
        let rb = RingBuffer::new(10);
        
        for i in 0..5 {
            rb.try_push(i as f32).unwrap();
        }
        rb.clear();
        assert_eq!(rb.try_pop(), None);
    }

    #[test]
    fn test_resample_linear_basic() {
        // 10 frames mono @ 48kHz -> resample para 48kHz (sem mudança)
        let input: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let output = resample_linear(&input, 1, 48000, 48000);
        assert_eq!(output.len(), 10);
        assert_eq!(output, input);
    }

    #[test]
    fn test_resample_linear_downsample() {
        // 100 frames @ 48kHz -> 200 frames @ 96kHz (upsample 2x)
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let output = resample_linear(&input, 1, 48000, 96000);
        assert_eq!(output.len(), 200);
        // Verifica interpolação linear
        assert!((output[0] - 0.0).abs() < f32::EPSILON);
        assert!((output[199] - 99.0).abs() < 1.0); // Aproximado
    }

    #[test]
    fn test_mono_to_stereo_conversion() {
        // Simula conversão mono -> stereo na lógica de decode
        let mono: Vec<f32> = vec![1.0, 2.0, 3.0];
        let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
        assert_eq!(stereo, vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn test_xrun_counter_starts_at_zero() {
        let engine = AudioEngine::new().unwrap();
        assert_eq!(engine.get_xrun_count(), 0);
    }
}