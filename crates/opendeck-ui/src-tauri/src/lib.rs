use opendeck_analysis::{analyze_audio, cache::AnalysisCache};
use opendeck_audio::{AudioEngine, PlaybackState};
use opendeck_core::{CuePoint, Track};
use opendeck_db::read_library;
use opendeck_hardware::UsbScanner;
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

mod state_persistence;
use state_persistence::{load_user_state, save_user_state, set_last_usb_root, UserState, UserPreferences};

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
    analysis_cache: AnalysisCache,
    library: Arc<Mutex<Vec<Track>>>,
    user_state: Mutex<UserState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            audio_engines: AudioEngineHandle::default(),
            analysis_cache: AnalysisCache::new(),
            library: Arc::new(Mutex::new(Vec::new())),
            user_state: Mutex::new(load_user_state().unwrap_or_default()),
        }
    }
}

/// Lê a biblioteca completa de um Pen Drive exportado pelo Rekordbox.
/// Inicia a análise de áudio em background para cada track que ainda não
/// tenha resultado em cache (disco ou memória).
#[tauri::command]
async fn load_library(
    usb_root: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Track>, String> {
    let usb_path = Path::new(&usb_root);
    let mut tracks = read_library(usb_path)
        .map_err(|e| format!("Erro ao ler biblioteca Rekordbox: {}", e))?;

    let cache = state.analysis_cache.clone();

    // Preenche analysis a partir do cache local (memória/disco) para cada track
    for track in tracks.iter_mut() {
        let audio_path = Path::new(&track.file_path);
        if audio_path.exists() {
            if let Some(cached) = cache.get(audio_path) {
                track.analysis = Some(cached);
            }
        }
    }

    // Salva a biblioteca no estado da aplicação para uso posterior
    {
        let mut lib = state.library.lock().map_err(|e| e.to_string())?;
        *lib = tracks.clone();
    }

    // Dispara análise em background para tracks sem cache
        let tracks_for_bg: Vec<Track> = tracks
            .iter()
            .filter(|t| t.analysis.is_none() && Path::new(&t.file_path).exists())
            .cloned()
            .collect();

        if !tracks_for_bg.is_empty() {
                    let cache_bg = cache.clone();
                    let lib_state = state.library.clone();
                    tauri::async_runtime::spawn(async move {
                        for track in tracks_for_bg {
                            let file_path = track.file_path.clone();
                            let track_id = track.id.clone();
                            let file_path_for_blocking = file_path.clone();
                            if let Ok(analysis) = spawn_blocking(move || {
                                let audio_path = Path::new(&file_path_for_blocking);
                                analyze_audio(audio_path)
                            }).await {
                                if let Ok(analysis) = analysis {
                                    let audio_path = Path::new(&file_path);
                                    let _ = cache_bg.insert(audio_path, analysis.clone());
                                    // Atualiza a biblioteca em memória
                                    if let Ok(mut lib) = lib_state.lock() {
                                        if let Some(t) = lib.iter_mut().find(|t| t.id == track_id) {
                                            t.analysis = Some(analysis);
                                        }
                                    }
                                }
                            }
                        }
                    });
                }

    // Salva último USB root usado
    {
        let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
        set_last_usb_root(&mut us, usb_root.clone());
        let us_clone = us.clone();
        drop(us);
        let _ = save_user_state(&us_clone);
    }

    Ok(tracks)
    }

    #[tauri::command]
    async fn load_track_analysis(file_path: String) -> Result<opendeck_core::TrackAnalysis, String> {
        let analysis = analyze_audio(Path::new(&file_path))
            .map_err(|e| format!("Erro ao analisar arquivo de áudio: {}", e))?;
        Ok(analysis)
    }

/// Retorna os cue points de uma track específica.
///
/// Os cue points já vêm embutidos na `Track` retornada por `load_library`
/// (extraídos dos nós `<POSITION_MARK>` do rekordbox.xml).
/// Mantido para compatibilidade com chamadas futuras do frontend.
#[tauri::command]
async fn get_cues_for_track(
    _track_id: String,
) -> Result<Vec<CuePoint>, String> {
    // Implementação futura: buscar cues da track já carregada em memória
    Ok(vec![])
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
        r"D:\PIONEER
ekordbox",
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

/// Carrega o estado persistido do usuário.
#[tauri::command]
async fn get_user_state(
    state: tauri::State<'_, AppState>,
) -> Result<UserState, String> {
    let user_state = state.user_state.lock().map_err(|e| e.to_string())?;
    Ok(user_state.clone())
}

/// Salva o estado do usuário (preferências, janela, etc.).
#[tauri::command]
async fn save_user_state_cmd(
    user_state: UserState,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
        *us = user_state.clone();
    }
    save_user_state(&user_state).map_err(|e| format!("Erro ao salvar estado: {}", e))
}

/// Atualiza preferências do usuário.
#[tauri::command]
async fn update_preferences(
    preferences: UserPreferences,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
        us.preferences = preferences.clone();
    }
    let us = {
        let guard = state.user_state.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    save_user_state(&us).map_err(|e| format!("Erro ao salvar preferências: {}", e))
}

/// Atualiza estado da janela (tamanho/posição).
#[tauri::command]
async fn update_window_state_cmd(
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
        us.window_width = Some(width);
        us.window_height = Some(height);
        us.window_x = Some(x);
        us.window_y = Some(y);
    }
    let us = {
        let guard = state.user_state.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    save_user_state(&us).map_err(|e| format!("Erro ao salvar estado da janela: {}", e))
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
            get_user_state,
            save_user_state_cmd,
            update_preferences,
            update_window_state_cmd,
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
