//! Estrutura de erros para o parser XML do Rekordbox.

use thiserror::Error;

/// Erros possíveis ao operar no parser de `rekordbox.xml`.
#[derive(Error, Debug)]
pub enum XmlParseError {
    #[error("Failed to read XML file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse XML: {0}")]
    XmlError(#[from] roxmltree::Error),

    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),

    #[error("Invalid attribute value for '{0}': {1}")]
    InvalidAttribute(String, String),

    #[error("Unsupported XML version or format")]
    UnsupportedFormat,
}
