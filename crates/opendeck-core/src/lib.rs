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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CueType {
    Normal = 0,
    LoopIn = 1,
    LoopOut = 2,
    Memory = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuePoint {
    pub id: String,
    pub track_id: String,
    pub time_seconds: f32,
    pub cue_type: CueType,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysis {
    pub beatgrid_seconds: Vec<f32>,
    pub waveform_amplitudes: Vec<u8>,
}

// Erros base que podem ser reutilizados
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Analysis error: {0}")]
    Analysis(String),
}