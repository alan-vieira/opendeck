//! Leitor da biblioteca Rekordbox via `rekordbox.xml`.
//!
//! Este crate substituiu o antigo leitor de `master.db` (SQLCipher) por um
//! parser de XML performático (`roxmltree`), lendo o arquivo exportado pelo
//! Rekordbox em um Pen Drive (`PIONEER/rekordbox/rekordbox.xml`).

pub use error::XmlParseError;
pub mod error;
pub mod parser;

use opendeck_core::Track;
use parser::RekordboxXmlParser;
use std::path::Path;

/// Lê a biblioteca completa de um Pen Drive exportado pelo Rekordbox.
///
/// Espera o arquivo em `<usb_root>/PIONEER/rekordbox/rekordbox.xml`.
pub fn read_library(usb_root: &Path) -> Result<Vec<Track>, XmlParseError> {
    RekordboxXmlParser::parse_tracks(
        &usb_root.join("PIONEER").join("rekordbox").join("rekordbox.xml"),
    )
}

/// Lê apenas as faixas de uma playlist específica.
///
/// Implementação futura: filtrar por `playlist_name` nos nós `<PLAYLIST>` do XML.
pub fn read_playlist(_usb_root: &Path, _playlist_name: &str) -> Result<Vec<Track>, XmlParseError> {
    Err(XmlParseError::UnsupportedFormat)
}
