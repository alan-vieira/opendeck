//! Motor de análise de áudio do OpenDeck.
//!
//! Substitui a leitura de arquivos `.DAT` (proprietários) por uma análise própria
//! de arquivos de áudio reais (MP3, WAV, FLAC, AAC) usando `symphonia` (100% Rust).

pub mod error;
pub mod analyzer;
pub mod cache;
pub mod parser;

use error::AnalysisError;
use opendeck_core::TrackAnalysis;
use std::path::Path;

// Re-exporta o parser de .DAT legado para retrocompatibilidade (a ser removido na Fase 3)
pub use parser::DatParser;

/// Analisa um arquivo de áudio real e retorna `TrackAnalysis`.
///
/// Este é o novo ponto de entrada da Fase 2, substituindo a leitura de `.DAT`.
pub fn analyze_audio(file_path: &Path) -> Result<TrackAnalysis, AnalysisError> {
    analyzer::AudioAnalyzer::analyze(file_path)
}

/// Wrapper de conveniência para manter a assinatura antiga (`parse_file`)
/// delegando para o novo analisador de áudio.
pub fn parse_file(file_path: &str) -> Result<TrackAnalysis, AnalysisError> {
    analyzer::AudioAnalyzer::analyze(Path::new(file_path))
}
