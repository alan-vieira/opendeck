// opendeck-core library// crates/opendeck-core/src/lib.rs
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub bpm: Option<f32>,
    pub file_path: String,
    pub analysis_path: Option<String>,
    pub cues: Vec<CuePoint>,
    pub analysis: Option<TrackAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CueType {
    Memory,
    HotCue,
    Loop,
    FadeIn,
    FadeOut,
    Unknown(u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuePoint {
    pub id: String,
    pub track_id: String,
    pub name: String,
    pub cue_type: CueType,
    pub start_ms: u32,
    pub end_ms: Option<u32>,
    pub hot_cue_index: Option<u8>,
    pub color_rgb: Option<(u8, u8, u8)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysis {
    pub beatgrid_seconds: Vec<f32>,
    pub waveform_amplitudes: Vec<u8>,
    pub estimated_bpm: Option<f32>,
    pub duration_seconds: f32, // 👈 NOVO CAMPO
}

// Erros base que podem ser reutilizados
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Analysis error: {0}")]
    Analysis(String),
}