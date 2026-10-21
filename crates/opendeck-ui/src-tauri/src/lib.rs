use opendeck_analysis::DatParser;
use opendeck_audio::{AudioEngine, PlaybackState};
use opendeck_core::{CuePoint, Track};
use opendeck_db::RekordboxDb;
use opendeck_hardware::UsbScanner;
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Wrapper para AudioEngine que implementa Send + Sync manualmente.
/// cpal::Stream não implementa Send/Sync, então não pode ser compartilhado
/// entre threads diretamente. Como o AudioEngine só é acessado através do
/// Mutex (exclusão mútua), é seguro wrapear com Send + Sync manual.
#[derive(Default)]
struct AudioEngineHandle(Arc<Mutex<[Option<AudioEngine>; 2]>>);

unsafe impl Send for AudioEngineHandle {}
unsafe impl Sync for AudioEngineHandle {}

impl AudioEngineHandle {
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, [Option<AudioEngine>; 2]>> {
        self.0.lock()
    }
}

/// Informação de dispositivo Pioneer (será serializada para o frontend).
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub bus_number: u8,
    pub device_address: u8,
    pub product_string: Option<String>,
    pub serial_number: Option<String>,
}

/// Estado global da aplicação.
struct AppState {
    audio_engines: AudioEngineHandle,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            audio_engines: AudioEngineHandle::default(),
        }
    }
}

/// Obtém a chave de criptografia do Rekordbox.
///
/// Prioridade: variável de ambiente `OPENDECK_DB_KEY` > fallback (texto plano,
/// apenas para desenvolvimento). Em Rekordbox 6.6.5+ a chave não é mais texto
/// plano no `app.asar`, então o usuário deve fornecê-la via variável de
/// ambiente ou tela de configuração.
fn db_key() -> String {
    std::env::var("OPENDECK_DB_KEY")
        .unwrap_or_else(|_| "402fd482c38817c35ffa8ffb8c7d93143b749e7d315df7a81732a1ff43608497".to_string())
}

#[tauri::command]
async fn load_library(db_path: String) -> Result<Vec<Track>, String> {
    let key = db_key();
    let db = RekordboxDb::open(&db_path, &key)
        .map_err(|e| format!("Erro ao abrir banco de dados: {}", e))?;
    let tracks = db.get_tracks().map_err(|e| format!("Erro ao ler tracks: {}", e))?;
    Ok(tracks)
}

#[tauri::command]
async fn load_track_analysis(dat_path: String) -> Result<opendeck_core::TrackAnalysis, String> {
    let analysis = DatParser::parse_file(&dat_path)
        .map_err(|e| format!("Erro ao analisar arquivo .DAT: {}", e))?;
    Ok(analysis)
}

#[tauri::command]
async fn get_cues_for_track(
    db_path: String,
    track_id: String,
) -> Result<Vec<CuePoint>, String> {
    let key = db_key();
    let db = RekordboxDb::open(&db_path, &key)
        .map_err(|e| format!("Erro ao abrir banco de dados: {}", e))?;
    let cues = db
        .get_cues_for_track(&track_id)
        .map_err(|e| format!("Erro ao ler cues: {}", e))?;
    Ok(cues)
}

#[tauri::command]
async fn play_track(
    file_path: String,
    deck_index: usize,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }

    // Resolve caminho absoluto: o Rekordbox armazena apenas o nome do arquivo
    // ou caminho relativo; tentamos diretórios de música comuns.
    let resolved = resolve_audio_path(&file_path);
    let final_path = resolved.as_deref().unwrap_or(&file_path);

    let mut engines = state.audio_engines.lock().map_err(|e| e.to_string())?;
    let engine = engines
        .get_mut(deck_index)
        .ok_or("Deck não encontrado")?;

    // Cria um novo engine para este deck se não existir
    if engine.is_none() {
        let mut new_engine = AudioEngine::new()
            .map_err(|e| format!("Erro ao criar motor de áudio: {}", e))?;
        new_engine
            .load_track(final_path)
            .map_err(|e| format!("Erro ao carregar track: {}", e))?;
        // Cria e inicia o stream CPAL (consumidor do ring buffer) antes de tocar.
        new_engine
            .start_stream()
            .map_err(|e| format!("Erro ao iniciar stream de áudio: {}", e))?;
        new_engine.play().map_err(|e| format!("Erro ao iniciar reprodução: {}", e))?;
        *engine = Some(new_engine);
    } else {
        let engine_ref = engine.as_mut().unwrap();
        engine_ref
            .load_track(final_path)
            .map_err(|e| format!("Erro ao carregar track: {}", e))?;
        if engine_ref.get_state() == PlaybackState::Stopped {
            engine_ref
                .start_stream()
                .map_err(|e| format!("Erro ao iniciar stream de áudio: {}", e))?;
        }
        engine_ref.play().map_err(|e| format!("Erro ao iniciar reprodução: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
async fn stop_track(deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }

    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.stop();
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_tempo(
    factor: f32,
    deck_index: usize,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.set_tempo(factor);
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado para este deck".to_string())
}

#[tauri::command]
async fn set_pitch(
    semitones: f32,
    deck_index: usize,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.set_pitch(semitones);
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado para este deck".to_string())
}

#[tauri::command]
async fn set_volume(
    volume: f32,
    deck_index: usize,
    _state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }
    // Volume é aplicado no frontend (ganho de saída); placeholder para compatibilidade
    let _ = volume;
    Ok(())
}

/// Tenta resolver um caminho de áudio relativo para um caminho absoluto,
/// procurando em diretórios de música comuns do Windows (incluindo subpastas).
fn resolve_audio_path(rel_path: &str) -> Option<String> {
    // Se já é absoluto e existe, retorna como está
    let p = std::path::Path::new(rel_path);
    if p.is_absolute() && p.exists() {
        return Some(rel_path.to_string());
    }

    let file_name = p.file_name()?.to_string_lossy().to_string();

    let search_dirs = [
        r"C:\Users\alsvi\Music",
        r"C:\Users\alsvi\Músicas",
        r"D:\Músicas",
        r"C:\Users\alsvi\Desktop",
        r"C:\Users\alsvi\Documents",
        r"C:\Users\alsvi\AppData\Roaming\Pioneer
 rekordbox",
    ];

    // Busca direta (arquivo na raiz da pasta)
    for dir in &search_dirs {
        let full = std::path::Path::new(dir).join(&file_name);
        if full.exists() {
            return Some(full.to_string_lossy().to_string());
        }
    }

    // Busca recursiva em subpastas (útil para D:\Músicas com álbuns organizados)
    for dir in &search_dirs {
        let dir_path = std::path::Path::new(dir);
        if !dir_path.exists() {
            continue;
        }
        if let Some(entry) = walkdir::WalkDir::new(dir_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .find(|e| e.file_name() == std::ffi::OsStr::new(&file_name))
        {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }

    None
}

#[tauri::command]
async fn get_devices() -> Result<Vec<DeviceInfo>, String> {
    let scanner = UsbScanner::new().map_err(|e| format!("Erro ao criar scanner USB: {}", e))?;
    let devices = scanner.scan().map_err(|e| format!("Erro ao escanear dispositivos: {}", e))?;
    Ok(devices
        .into_iter()
        .map(|d| DeviceInfo {
            vid: d.vid,
            pid: d.pid.as_u16(),
            bus_number: d.bus_number,
            device_address: d.device_address,
            product_string: d.product_string,
            serial_number: d.serial_number,
        })
        .collect())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_library,
            load_track_analysis,
            get_cues_for_track,
            play_track,
            stop_track,
            set_tempo,
            set_pitch,
            set_volume,
            get_devices,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
