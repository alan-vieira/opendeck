//! Estrutura de erros para o motor de análise de áudio (`opendeck-analysis`).

use thiserror::Error;

/// Erros possíveis ao operar o analisador de áudio.
#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("Failed to open audio file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to decode audio: {0}")]
    DecodeError(String),

    #[error("Unsupported audio format or codec")]
    UnsupportedFormat,

    #[error("Audio file is too short to analyze")]
    TooShort,
}
