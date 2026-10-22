//! Persistência de estado do usuário (último USB, tracks recentes, preferências).
//!
//! Salva em `<config_dir>/opendeck/user_state.json`.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::{fs, io};

const MAX_RECENT_TRACKS: usize = 50;

/// Caminho do arquivo de estado do usuário.
fn state_path() -> io::Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "config dir not found"))?
        .join("opendeck");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("user_state.json"))
}

/// Estado persistido do usuário.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserState {
    /// Último USB root usado para carregar biblioteca.
    pub last_usb_root: Option<String>,

    /// Tracks recentes (MRU - Most Recently Used).
    /// Cada entrada: (track_id, title, artist, file_path, timestamp)
    pub recent_tracks: VecDeque<RecentTrack>,

    /// Último tamanho da janela.
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,

    /// Última posição da janela.
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,

    /// Preferências do usuário.
    pub preferences: UserPreferences,
}

/// Track recente para histórico MRU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentTrack {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub file_path: String,
    pub timestamp: u64, // Unix timestamp
}

/// Preferências do usuário.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPreferences {
    /// Volume mestre padrão (0.0 - 1.0).
    pub master_volume: f32,

    /// Tema preferido (light/dark/system).
    pub theme: String,

    /// Auto-analisar tracks ao carregar biblioteca.
    pub auto_analyze: bool,

    /// Tamanho padrão do buffer de áudio (segundos).
    pub audio_buffer_seconds: f32,

    /// Mostrar waveform por padrão.
    pub show_waveform: bool,

    /// Deck padrão para carregar (0 ou 1).
    pub default_deck: usize,
}

/// Carrega o estado do usuário do disco.
pub fn load_user_state() -> io::Result<UserState> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(UserState::default());
    }
    let json = fs::read_to_string(path)?;
    let state: UserState = serde_json::from_str(&json)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(state)
}

/// Salva o estado do usuário no disco.
pub fn save_user_state(state: &UserState) -> io::Result<()> {
    let path = state_path()?;
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(path, json)
}

/// Adiciona uma track ao histórico de recentes (MRU).
pub fn add_recent_track(state: &mut UserState, track_id: String, title: String, artist: String, file_path: String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    // Remove se já existe
    state.recent_tracks.retain(|t| t.track_id != track_id);

    // Adiciona no início
    state.recent_tracks.push_front(RecentTrack {
        track_id,
        title,
        artist,
        file_path,
        timestamp,
    });

    // Limita tamanho
    while state.recent_tracks.len() > MAX_RECENT_TRACKS {
        state.recent_tracks.pop_back();
    }
}

/// Define o último USB root usado.
pub fn set_last_usb_root(state: &mut UserState, usb_root: String) {
    state.last_usb_root = Some(usb_root);
}

/// Atualiza preferências do usuário.
pub fn update_preferences(state: &mut UserState, preferences: UserPreferences) {
    state.preferences = preferences;
}

/// Atualiza estado da janela.
pub fn update_window_state(state: &mut UserState, width: u32, height: u32, x: i32, y: i32) {
    state.window_width = Some(width);
    state.window_height = Some(height);
    state.window_x = Some(x);
    state.window_y = Some(y);
}