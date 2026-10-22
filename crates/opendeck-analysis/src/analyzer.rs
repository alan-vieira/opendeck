//! Analisador de áudio próprio do OpenDeck.

use crate::error::AnalysisError;
use opendeck_core::TrackAnalysis;
use rustfft::FftPlanner;
use rustfft::FftDirection;
use rustfft::num_complex::Complex;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub beatgrid_seconds: Vec<f32>,
    pub waveform_amplitudes: Vec<f32>,
    pub sample_rate: u32,
    pub duration_seconds: f32,
    pub bpm_estimate: Option<f32>,
}

pub struct AudioAnalyzer;

impl AudioAnalyzer {
    pub fn analyze(file_path: &Path) -> Result<TrackAnalysis, AnalysisError> {
        let result = Self::analyze_full(file_path)?;
        Ok(TrackAnalysis {
            beatgrid_seconds: result.beatgrid_seconds,
            waveform_amplitudes: result
                .waveform_amplitudes
                .into_iter()
                // 👇 Multiplicamos por um boost (2.5) pra waveform ficar mais visível com RMS
                .map(|s| (s * 255.0 * 2.5).clamp(0.0, 255.0) as u8)
                .collect(),
            estimated_bpm: result.bpm_estimate,
            duration_seconds: result.duration_seconds,
        })
    }

    pub fn analyze_full(file_path: &Path) -> Result<AnalysisResult, AnalysisError> {
        let file = std::fs::File::open(file_path).map_err(AnalysisError::IoError)?;
        let mss = symphonia::core::io::MediaSourceStream::new(
            Box::new(file),
            symphonia::core::io::MediaSourceStreamOptions::default(),
        );

        let mut hint = Hint::new();
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probe = get_probe();
        let probed = probe
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|_| AnalysisError::UnsupportedFormat)?;

        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or(AnalysisError::UnsupportedFormat)?;
        let track_id = track.id;

        let codec_registry = get_codecs();
        let params = track.codec_params.clone();
        let mut decoder = codec_registry
            .make(&params, &DecoderOptions::default())
            .map_err(|e| AnalysisError::DecodeError(e.to_string()))?;

        let mut all_samples: Vec<f32> = Vec::new();
        let mut sample_rate: Option<u32> = None;
        let mut total_frames: u64 = 0;
        let mut sample_buf: Option<SampleBuffer<f32>> = None;

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(_) => break,
            };
            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Pacote inválido (ignorando): {}", e);
                    continue;
                }
            };

            let spec = *decoded.spec();
            if sample_rate.is_none() {
                sample_rate = Some(spec.rate);
            }

            let channels = spec.channels.count();
            let frame_count = decoded.frames();
            total_frames += frame_count as u64;

            let buf = sample_buf.get_or_insert_with(|| {
                SampleBuffer::<f32>::new(frame_count as u64, spec)
            });
            if buf.capacity() < frame_count {
                *buf = SampleBuffer::<f32>::new(frame_count as u64, spec);
            }
            buf.copy_interleaved_ref(decoded);
            let interleaved = buf.samples();

            if channels > 0 {
                let mut mono_sum = vec![0.0f32; frame_count];
                for frame in 0..frame_count {
                    let mut sum = 0.0f32;
                    for ch in 0..channels {
                        let idx = frame * channels + ch;
                        if idx < interleaved.len() {
                            sum += interleaved[idx];
                        }
                    }
                    mono_sum[frame] = sum / channels as f32;
                }
                all_samples.extend_from_slice(&mono_sum);
            }
        }

        let sample_rate = sample_rate.ok_or(AnalysisError::UnsupportedFormat)?;
        if all_samples.is_empty() {
            return Err(AnalysisError::TooShort);
        }

        let duration_seconds = total_frames as f32 / sample_rate as f32;

        let display_samples = 2048;
        let waveform = Self::downsample_waveform(&all_samples, display_samples);

        let bpm_estimate = Self::estimate_bpm_fft(&all_samples, sample_rate)
            .or_else(|| Self::estimate_bpm_rms(&all_samples, sample_rate));

        let beatgrid_seconds = Self::compute_beatgrid(&all_samples, sample_rate, bpm_estimate);

        Ok(AnalysisResult {
            beatgrid_seconds,
            waveform_amplitudes: waveform,
            sample_rate,
            duration_seconds,
            bpm_estimate,
        })
    }

    // 👇 MUDANÇA: usa RMS em vez de pico máximo
    fn downsample_waveform(samples: &[f32], target_len: usize) -> Vec<f32> {
        if samples.is_empty() || target_len == 0 {
            return Vec::new();
        }

        let len = samples.len();
        let mut out = Vec::with_capacity(target_len);
        let step = len as f32 / target_len as f32;

        for i in 0..target_len {
            let start = ((i as f32) * step) as usize;
            let end = (((i + 1) as f32) * step) as usize;
            let end = end.min(len).max(start + 1);
            let window = &samples[start..end];

            // 👇 MUDANÇA: RMS (energia média) em vez de pico máximo
            let sum_sq: f32 = window.iter().map(|s| s * s).sum();
            let rms = (sum_sq / window.len() as f32).sqrt();
            out.push(rms);
        }

        out
    }

    fn estimate_bpm_fft(samples: &[f32], sample_rate: u32) -> Option<f32> {
        if samples.len() < 1024 {
            return None;
        }

        let target_rate: u32 = 1000;
        let ratio = sample_rate as f32 / target_rate as f32;
        let downsampled_len = (samples.len() as f32 / ratio) as usize;
        let mut downsampled: Vec<f32> = Vec::with_capacity(downsampled_len);
        for i in 0..downsampled_len {
            let src_idx = (i as f32 * ratio) as usize;
            downsampled.push(samples.get(src_idx).copied().unwrap_or(0.0));
        }

        let mut fft_size = 1;
        while fft_size < downsampled.len() * 2 {
            fft_size *= 2;
        }
        if fft_size < 1024 {
            fft_size = 1024;
        }

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft(fft_size, FftDirection::Forward);
        let ifft = planner.plan_fft_inverse(fft_size);

        let mut input: Vec<Complex<f32>> = downsampled
            .iter()
            .chain(std::iter::repeat(&0.0f32))
            .take(fft_size)
            .map(|&s| Complex::new(s, 0.0))
            .collect();

        fft.process(input.as_mut_slice());
        for c in input.iter_mut() {
            *c = *c * c.conj();
        }
        ifft.process(input.as_mut_slice());

        let autocorr: Vec<f32> = input.iter().map(|c| c.re / fft_size as f32).collect();

        let min_lag = (0.3 * target_rate as f32) as usize;
        let max_lag = (1.0 * target_rate as f32) as usize;
        if max_lag >= autocorr.len() {
            return None;
        }

        let mut best_lag = 0;
        let mut best_val = f32::NEG_INFINITY;
        for lag in min_lag..=max_lag {
            if autocorr[lag] > best_val {
                best_val = autocorr[lag];
                best_lag = lag;
            }
        }

        if best_lag == 0 || best_val <= 0.0 {
            return None;
        }

        let interpolated_lag = if best_lag > min_lag && best_lag < max_lag {
            let y0 = autocorr[best_lag - 1];
            let y1 = autocorr[best_lag];
            let y2 = autocorr[best_lag + 1];
            let denom = (y0 - 2.0 * y1 + y2).abs();
            if denom > f32::EPSILON {
                let offset = 0.5 * (y0 - y2) / denom;
                if offset.abs() <= 1.0 {
                    best_lag as f32 + offset
                } else {
                    best_lag as f32
                }
            } else {
                best_lag as f32
            }
        } else {
            best_lag as f32
        };

        let interval_seconds = interpolated_lag / target_rate as f32;
        if interval_seconds <= 0.0 {
            return None;
        }

        let bpm = 60.0 / interval_seconds;
        if bpm < 60.0 || bpm > 200.0 {
            return None;
        }

        Some(bpm)
    }

    fn estimate_bpm_rms(samples: &[f32], sample_rate: u32) -> Option<f32> {
        if samples.is_empty() {
            return None;
        }

        let window_ms = 50u32;
        let window_size = (sample_rate as f32 * window_ms as f32 / 1000.0) as usize;
        if window_size == 0 || samples.len() < window_size * 2 {
            return None;
        }

        let mut energy: Vec<f32> = Vec::new();
        let mut i = 0;
        while i + window_size <= samples.len() {
            let window = &samples[i..i + window_size];
            let sum_sq: f32 = window.iter().map(|s| s * s).sum();
            let rms = (sum_sq / window_size as f32).sqrt();
            energy.push(rms);
            i += window_size / 2;
        }

        if energy.len() < 4 {
            return None;
        }

        let mean: f32 = energy.iter().sum::<f32>() / energy.len() as f32;
        let std: f32 = (energy.iter().map(|e| (e - mean).powi(2)).sum::<f32>() / energy.len() as f32).sqrt();
        let threshold = mean + std * 0.8;

        let mut peak_indices: Vec<usize> = Vec::new();
        for idx in 1..energy.len() - 1 {
            if energy[idx] > threshold && energy[idx] >= energy[idx - 1] && energy[idx] >= energy[idx + 1] {
                peak_indices.push(idx);
            }
        }

        if peak_indices.len() < 4 {
            return None;
        }

        let hop_samples = window_size / 2;
        let seconds_per_index = hop_samples as f32 / sample_rate as f32;
        let peak_times: Vec<f32> = peak_indices.iter().map(|&idx| idx as f32 * seconds_per_index).collect();

        let mut interval_histogram: Vec<(f32, usize)> = Vec::new();
        for i in 1..peak_times.len() {
            let interval = peak_times[i] - peak_times[i - 1];
            if interval > 0.3 && interval < 1.0 {
                let bucket_idx = interval_histogram.iter().position(|(b, _)| (b - interval).abs() < 0.02);
                match bucket_idx {
                    Some(idx) => interval_histogram[idx].1 += 1,
                    None => interval_histogram.push((interval, 1)),
                }
            }
        }

        if interval_histogram.is_empty() {
            return None;
        }

        interval_histogram.sort_by(|a, b| b.1.cmp(&a.1));
        let best_interval = interval_histogram[0].0;

        let bpm = 60.0 / best_interval;
        if bpm < 60.0 || bpm > 200.0 {
            return None;
        }

        Some(bpm)
    }

    fn compute_beatgrid(samples: &[f32], sample_rate: u32, bpm: Option<f32>) -> Vec<f32> {
        let bpm = match bpm {
            Some(b) => b,
            None => return Vec::new(),
        };

        let sec_per_beat = 60.0 / bpm;
        let duration = samples.len() as f32 / sample_rate as f32;
        let mut grid: Vec<f32> = Vec::new();

        let mut t = 0.0;
        while t < duration {
            grid.push(t);
            t += sec_per_beat;
        }

        grid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_downsample_waveform() {
        let samples: Vec<f32> = (0..100).map(|i| (i as f32).sin()).collect();
        let downsampled = AudioAnalyzer::downsample_waveform(&samples, 10);
        assert_eq!(downsampled.len(), 10);
        for v in &downsampled {
            assert!((0.0..=1.0).contains(v));
        }
    }

    #[test]
    fn test_analyze_nonexistent_file() {
        let path = PathBuf::from("/caminho/inexistente/audio.mp3");
        let err = AudioAnalyzer::analyze(&path).unwrap_err();
        assert!(matches!(err, AnalysisError::IoError(_)));
    }

    #[test]
    fn test_estimate_bpm_steady_beat() {
        let sample_rate: u32 = 44100;
        let bpm_target = 128.0;
        let sec_per_beat = 60.0 / bpm_target;
        let samples_per_beat = (sample_rate as f32 * sec_per_beat) as usize;
        let duration_beats = 32;
        let mut samples: Vec<f32> = Vec::new();

        for beat in 0..duration_beats {
            let start = beat * samples_per_beat;
            for i in 0..samples_per_beat {
                let t = i as f32 / sample_rate as f32;
                let envelope = (-t * 10.0).exp();
                let sample = 0.8 * envelope * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                samples.push(sample);
            }
            for _ in start + samples_per_beat..start + samples_per_beat * 2 {
                if samples.len() < start + samples_per_beat * 2 {
                    samples.push(0.001);
                }
            }
        }

        let bpm = AudioAnalyzer::estimate_bpm_fft(&samples, sample_rate)
            .or_else(|| AudioAnalyzer::estimate_bpm_rms(&samples, sample_rate));
        assert!(bpm.is_some(), "BPM deve ser estimado para sinal com batida regular");
        let bpm = bpm.unwrap();
        assert!((bpm - bpm_target).abs() < 5.0, "BPM estimado {:?} muito distante do alvo {}", bpm, bpm_target);
    }

    #[test]
    fn test_estimate_bpm_too_short() {
        let samples: Vec<f32> = vec![0.1; 100];
        let bpm = AudioAnalyzer::estimate_bpm_fft(&samples, 44100);
        assert!(bpm.is_none(), "Áudio muito curto não deve gerar BPM");
    }
}