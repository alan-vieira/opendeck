//! Cache local de análises de áudio.
//!
//! Salva o resultado de `TrackAnalysis` em arquivos JSON no diretório
//! de cache do usuário, indexados pelo caminho do arquivo de áudio.
//! Evita re-decodificar o mesmo arquivo a cada reinicialização.

use opendeck_core::TrackAnalysis;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fs, io};

/// Diretório de cache: `<cache_dir>/opendeck/analysis/`
fn cache_dir() -> io::Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cache dir not found"))?
        .join("opendeck")
        .join("analysis");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Chave do cache: hash simples do caminho canônico do arquivo.
fn cache_key(file_path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    file_path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Caminho do arquivo de cache para um dado arquivo de áudio.
fn cache_path(file_path: &Path) -> io::Result<PathBuf> {
    Ok(cache_dir()?.join(format!("{}.json", cache_key(file_path))))
}

/// Salva uma análise no cache local.
pub fn save_analysis(file_path: &Path, analysis: &TrackAnalysis) -> io::Result<()> {
    let path = cache_path(file_path)?;
    let json = serde_json::to_string_pretty(analysis)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(path, json)
}

/// Carrega uma análise do cache local, se existir.
pub fn load_analysis(file_path: &Path) -> io::Result<Option<TrackAnalysis>> {
    let path = match cache_path(file_path) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(path)?;
    let analysis: TrackAnalysis =
        serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(Some(analysis))
}

/// Remove uma análise do cache (útil ao trocar de faixa).
pub fn remove_analysis(file_path: &Path) -> io::Result<()> {
    if let Ok(path) = cache_path(file_path) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

/// Cache em memória para evitar leituras repetidas de disco durante uma sessão.
#[derive(Default)]
#[derive(Clone)]
pub struct AnalysisCache {
    inner: Arc<Mutex<HashMap<String, TrackAnalysis>>>,
}

impl AnalysisCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tenta carregar do cache em memória, senão do disco, senão None.
    pub fn get(&self, file_path: &Path) -> Option<TrackAnalysis> {
        let key = cache_key(file_path);
        let guard = self.inner.lock().ok()?;
        guard.get(&key).cloned()
    }

    /// Insere na memória e persiste no disco.
    pub fn insert(&self, file_path: &Path, analysis: TrackAnalysis) -> io::Result<()> {
        let key = cache_key(file_path);
        {
            let mut guard = self.inner.lock().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            guard.insert(key, analysis.clone());
        }
        save_analysis(file_path, &analysis)
    }
}
