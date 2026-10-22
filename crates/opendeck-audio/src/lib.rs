//! Motor de áudio de baixa latência para OpenDeck.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, SampleRate, StreamConfig, PlayStreamError};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use thiserror::Error;

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub const BUFFER_SIZE: u32 = 128;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

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

#[cfg(target_os = "windows")]
fn prefer_asio() -> bool { true }
#[cfg(target_os = "linux")]
fn prefer_asio() -> bool { false }

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

    pub fn try_push(&self, sample: f32) -> Result<(), f32> {
        let write = self.write_idx.load(Ordering::Relaxed);
        let next_write = (write + 1) % self.capacity;
        let read = self.read_idx.load(Ordering::Acquire);
        if next_write == read { return Err(sample); }
        unsafe { let buf = &mut *self.buffer.get(); buf[write] = sample; }
        self.write_idx.store(next_write, Ordering::Release);
        Ok(())
    }

    pub fn try_pop(&self) -> Option<f32> {
        let read = self.read_idx.load(Ordering::Relaxed);
        let write = self.write_idx.load(Ordering::Acquire);
        if read == write { return None; }
        let sample = unsafe { let buf = &*self.buffer.get(); buf[read] };
        self.read_idx.store((read + 1) % self.capacity, Ordering::Release);
        Some(sample)
    }

    pub fn clear(&self) {
        self.read_idx.store(0, Ordering::Release);
        self.write_idx.store(0, Ordering::Release);
    }

    pub fn capacity(&self) -> usize { self.capacity }

    pub fn available(&self) -> usize {
        let write = self.write_idx.load(Ordering::Acquire);
        let read = self.read_idx.load(Ordering::Acquire);
        (write + self.capacity - read) % self.capacity
    }
}

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
    volume: Arc<Mutex<f32>>,
    decode_handle: Option<thread::JoinHandle<()>>,
    decode_shutdown: Arc<Mutex<bool>>,
    current_path: Arc<Mutex<Option<String>>>,
    seek_request: Arc<Mutex<Option<u64>>>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, AudioError> {
        Self::with_buffer_seconds(AudioEngineConfig::default(), 2.0)
    }

    pub fn with_buffer_seconds(config: AudioEngineConfig, buffer_seconds: f32) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = if config.prefer_asio {
            host.output_devices()
                .ok()
                .and_then(|mut iter| iter.find(|d| d.name().ok().map(|n| n.to_lowercase().contains("asio")).unwrap_or(false)))
                .or_else(|| host.default_output_device())
        } else {
            host.default_output_device()
        };
        let device = device.ok_or_else(|| AudioError::DeviceNotFound("Nenhum dispositivo de saída disponível".to_string()))?;
        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        let config_range = device.supported_output_configs()
            .map_err(|e| AudioError::StreamError(e.to_string()))?
            .find(|c| c.channels() >= 2 && c.sample_format() == SampleFormat::F32)
            .ok_or_else(|| AudioError::UnsupportedSampleFormat("Dispositivo não suporta f32 stereo".to_string()))?;

        let mut stream_config: StreamConfig = config_range.with_sample_rate(SampleRate(config.sample_rate)).config();
        stream_config.buffer_size = BufferSize::Fixed(config.buffer_size);

        let ring_frames = (config.sample_rate as f32 * buffer_seconds) as usize;
        let ring_buffer = Arc::new(RingBuffer::new(ring_frames * stream_config.channels as usize));

        Ok(Self {
            config: config.clone(),
            state: Arc::new(Mutex::new(PlaybackState::Stopped)),
            stream: None,
            device_name,
            sample_rate: config.sample_rate,
            channels: stream_config.channels,
            ring_buffer,
            xrun_count: Arc::new(Mutex::new(0u64)),
            tempo_factor: Arc::new(Mutex::new(1.0)),
            pitch_semitones: Arc::new(Mutex::new(0.0)),
            volume: Arc::new(Mutex::new(1.0)),
            decode_handle: None,
            decode_shutdown: Arc::new(Mutex::new(false)),
            current_path: Arc::new(Mutex::new(None)),
            seek_request: Arc::new(Mutex::new(None)),
        })
    }

    pub fn start_stream(&mut self) -> Result<(), AudioError> {
        if self.stream.is_some() { return Ok(()); }
        let ring_buffer = Arc::clone(&self.ring_buffer);
        let xrun_count = Arc::clone(&self.xrun_count);
        let state = Arc::clone(&self.state);
        let volume = Arc::clone(&self.volume);
        let channels = self.channels;

        let stream = self.build_output_stream(ring_buffer, xrun_count, state, volume, channels)?;
        stream.play().map_err(|e: PlayStreamError| AudioError::StreamError(e.to_string()))?;
        self.stream = Some(stream);
        *self.state.lock().unwrap() = PlaybackState::Playing;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), AudioError> { self.start_stream() }

    pub fn play_file(&mut self, path: &Path) -> Result<(), AudioError> {
        self.load_track(path.to_str().ok_or_else(|| AudioError::FileNotFound(path.display().to_string()))?)?;
        if self.stream.is_none() { self.start_stream()?; }
        self.play()?;
        Ok(())
    }

    pub fn shutdown(&mut self) {
        if let Some(stream) = self.stream.take() { let _ = stream.pause(); }
        *self.decode_shutdown.lock().unwrap() = true;
        if let Some(handle) = self.decode_handle.take() { let _ = handle.join(); }
        *self.state.lock().unwrap() = PlaybackState::Stopped;
    }

    fn build_output_stream(
        &self,
        ring_buffer: Arc<RingBuffer>,
        xrun_count: Arc<Mutex<u64>>,
        state: Arc<Mutex<PlaybackState>>,
        volume: Arc<Mutex<f32>>,
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

        let ch_count = channels as usize;

        device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let current_state = *state.lock().unwrap();
                let vol = *volume.lock().unwrap();

                if current_state != PlaybackState::Playing {
                    data.fill(0.0);
                    return;
                }

                // Consome samples do ring buffer e aplica ganho de volume.
                // O ring buffer já vem em estéreo intercalado (L, R, L, R...).
                for i in 0..data.len() {
                    match ring_buffer.try_pop() {
                        Some(s) => data[i] = s * vol,
                        None => {
                            data[i] = 0.0;
                            *xrun_count.lock().unwrap() += 1;
                        }
                    }
                }
                let _ = ch_count;
            },
            move |err| eprintln!("Audio stream error: {}", err),
            None,
        )
    }

    pub fn load_track(&mut self, path: &str) -> Result<(), AudioError> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(AudioError::FileNotFound(path.display().to_string()));
        }

        *self.decode_shutdown.lock().unwrap() = true;
        if let Some(handle) = self.decode_handle.take() { let _ = handle.join(); }
        *self.decode_shutdown.lock().unwrap() = false;
        self.ring_buffer.clear();
        *self.current_path.lock().unwrap() = Some(path.to_string_lossy().to_string());

        let ring_buffer = Arc::clone(&self.ring_buffer);
        let decode_shutdown = Arc::clone(&self.decode_shutdown);
        let seek_request = Arc::clone(&self.seek_request);
        let path = path.to_path_buf();
        let target_sample_rate = self.sample_rate;
        let target_channels = self.channels;

        let handle = thread::spawn(move || {
            if let Err(e) = decode_and_feed(
                &path,
                ring_buffer,
                decode_shutdown,
                seek_request,
                target_sample_rate,
                target_channels,
            ) {
                eprintln!("Decode error: {}", e);
            }
        });

        self.decode_handle = Some(handle);
        Ok(())
    }

    /// Pula para uma posição específica (em milissegundos) na track atual.
    pub fn seek(&mut self, position_ms: u64) -> Result<(), AudioError> {
        let path = self.current_path.lock().unwrap().clone()
            .ok_or_else(|| AudioError::FileNotFound("Nenhuma track carregada".into()))?;

        // Sinaliza shutdown da thread atual
        *self.decode_shutdown.lock().unwrap() = true;
        if let Some(handle) = self.decode_handle.take() { let _ = handle.join(); }
        *self.decode_shutdown.lock().unwrap() = false;
        self.ring_buffer.clear();

        // Cria nova thread que abre o arquivo no timestamp alvo
        let ring_buffer = Arc::clone(&self.ring_buffer);
        let decode_shutdown = Arc::clone(&self.decode_shutdown);
        let seek_request = Arc::clone(&self.seek_request);
        let target_sample_rate = self.sample_rate;
        let target_channels = self.channels;
        let start_ms = position_ms;

        let handle = thread::spawn(move || {
            if let Err(e) = decode_and_feed_from(
                Path::new(&path),
                ring_buffer,
                decode_shutdown,
                seek_request,
                target_sample_rate,
                target_channels,
                start_ms,
            ) {
                eprintln!("Seek decode error: {}", e);
            }
        });
        self.decode_handle = Some(handle);
        Ok(())
    }

    pub fn play(&self) -> Result<(), AudioError> {
        if self.stream.is_none() { return Err(AudioError::ThreadNotRunning); }
        *self.state.lock().unwrap() = PlaybackState::Playing;
        Ok(())
    }

    pub fn pause(&self) { *self.state.lock().unwrap() = PlaybackState::Paused; }
    pub fn stop(&self) {
        *self.state.lock().unwrap() = PlaybackState::Stopped;
        self.ring_buffer.clear();
    }

    pub fn set_tempo(&mut self, factor: f32) {
        *self.tempo_factor.lock().unwrap() = factor.clamp(0.25, 4.0);
    }
    pub fn set_pitch(&mut self, semitones: f32) {
        *self.pitch_semitones.lock().unwrap() = semitones.clamp(-24.0, 24.0);
    }
    pub fn set_volume(&mut self, vol: f32) {
        *self.volume.lock().unwrap() = vol.clamp(0.0, 1.0);
    }
    pub fn get_xrun_count(&self) -> u64 { *self.xrun_count.lock().unwrap() }
    pub fn get_state(&self) -> PlaybackState { *self.state.lock().unwrap() }
    pub fn buffer_fill_seconds(&self) -> f32 {
        let available = self.ring_buffer.available();
        let frames = available / self.channels as usize;
        frames as f32 / self.sample_rate as f32
    }
    pub fn device_name(&self) -> &str { &self.device_name }
    pub fn sample_rate(&self) -> u32 { self.sample_rate }
    pub fn buffer_size(&self) -> u32 { self.config.buffer_size }
    pub fn theoretical_latency_ms(&self) -> f32 {
        (self.config.buffer_size as f32 / self.sample_rate as f32) * 1000.0
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) { self.shutdown(); }
}

fn decode_and_feed(
    path: &Path,
    ring_buffer: Arc<RingBuffer>,
    decode_shutdown: Arc<Mutex<bool>>,
    seek_request: Arc<Mutex<Option<u64>>>,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<(), AudioError> {
    decode_and_feed_from(path, ring_buffer, decode_shutdown, seek_request, target_sample_rate, target_channels, 0)
}

fn decode_and_feed_from(
    path: &Path,
    ring_buffer: Arc<RingBuffer>,
    decode_shutdown: Arc<Mutex<bool>>,
    seek_request: Arc<Mutex<Option<u64>>>,
    target_sample_rate: u32,
    target_channels: u16,
    start_ms: u64,
) -> Result<(), AudioError> {
    let source = std::fs::File::open(path).map_err(|e| AudioError::DecodeError(e.to_string()))?;
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
        .ok_or_else(|| AudioError::DecodeError("Nenhuma track de áudio válida".to_string()))?
        .clone();

    let decoder_opts = DecoderOptions { verify: false, ..Default::default() };
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|e| AudioError::DecodeError(format!("Decoder creation failed: {}", e)))?;

    let track_id = track.id;
    let time_base = track.codec_params.time_base;

    // Se start_ms > 0, faz seek no formato antes de começar a decodificar
    if start_ms > 0 {
        let seconds = start_ms as f64 / 1000.0;
        let time = Time::from(seconds);
        let seek_to = SeekTo::Time { time, track_id: Some(track_id) };
        let _ = format.seek(SeekMode::Accurate, seek_to);
        // Descarta frames até atingir o tempo exato
        loop {
            if *decode_shutdown.lock().unwrap() { return Ok(()); }
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(_) => break,
            };
            if packet.track_id() != track_id { continue; }
            if let Some(tb) = time_base {
                let ts = tb.calc_time(packet.ts());
                let current_ms = ((ts.seconds as f64) * 1000.0) as u64 + ((ts.frac as f64) * 1000.0) as u64;
                if current_ms >= start_ms { 
                    // Decodifica esse primeiro packet válido e começa o loop principal
                    if let Ok(decoded) = decoder.decode(&packet) {
                        feed_decoded(decoded, &ring_buffer, decode_shutdown.clone(), target_sample_rate, target_channels)?;
                    }
                    break; 
                }
            } else {
                break;
            }
        }
    }

    loop {
        if *decode_shutdown.lock().unwrap() { break; }

        // 👇 CORREÇÃO APLICADA AQUI: escopo isolado para dropar o MutexGuard antes do return
        let pending_seek = {
            let mut guard = seek_request.lock().unwrap();
            guard.take()
        };
        if let Some(pos) = pending_seek {
            return decode_and_feed_from(
                path,
                ring_buffer,
                decode_shutdown,
                seek_request,
                target_sample_rate,
                target_channels,
                pos,
            );
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(AudioError::DecodeError(format!("Packet error: {}", e))),
        };
        if packet.track_id() != track_id { continue; }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(e) => { eprintln!("Decode frame error: {}", e); continue; }
        };

        feed_decoded(decoded, &ring_buffer, decode_shutdown.clone(), target_sample_rate, target_channels)?;
    }

    Ok(())
}

fn feed_decoded(
    decoded: symphonia::core::audio::AudioBufferRef,
    ring_buffer: &Arc<RingBuffer>,
    decode_shutdown: Arc<Mutex<bool>>,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<(), AudioError> {
    let spec = *decoded.spec();
    let ch = spec.channels.count();
    let frames = decoded.frames();
    let rate = spec.rate;

    let mut sample_buf = SampleBuffer::<f32>::new(frames as u64, spec);
    sample_buf.copy_interleaved_ref(decoded);
    let interleaved = sample_buf.samples();

    let stereo: Vec<f32> = match (ch, target_channels) {
        (1, 2) => interleaved.iter().flat_map(|&s| [s, s]).collect(),
        (2, 2) => interleaved.to_vec(),
        (n, 2) if n > 2 => interleaved.chunks(n).flat_map(|c| [c[0], c[1]]).collect(),
        _ => interleaved.to_vec(),
    };

    let final_samples = if rate != target_sample_rate {
        resample_interleaved(&stereo, 2, rate, target_sample_rate)
    } else {
        stereo
    };

    for sample in final_samples {
        while ring_buffer.try_push(sample).is_err() {
            if *decode_shutdown.lock().unwrap() { return Ok(()); }
            thread::sleep(Duration::from_millis(1));
        }
    }
    Ok(())
}

fn resample_interleaved(input: &[f32], channels: usize, from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate { return input.to_vec(); }
    let ratio = from_rate as f32 / to_rate as f32;
    let in_frames = input.len() / channels;
    let out_frames = (in_frames as f32 / ratio) as usize;
    let mut output = Vec::with_capacity(out_frames * channels);
    for out_frame in 0..out_frames {
        let in_pos = out_frame as f32 * ratio;
        let in_frame = in_pos as usize;
        let frac = in_pos - in_frame as f32;
        let idx1_frame = (in_frame + 1).min(in_frames.saturating_sub(1));
        for c in 0..channels {
            let s0 = input[in_frame * channels + c];
            let s1 = input[idx1_frame * channels + c];
            output.push(s0 + frac * (s1 - s0));
        }
    }
    output
}