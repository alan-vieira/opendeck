use opendeck_analysis::{analyze_audio, cache::AnalysisCache};
use opendeck_audio::{AudioEngine, PlaybackState};
use opendeck_core::{CuePoint, Track};
use opendeck_db::read_library;
use opendeck_hardware::UsbScanner;
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};

mod state_persistence;
use state_persistence::{load_user_state, save_user_state, set_last_usb_root, UserState, UserPreferences};

#[derive(Default)]
struct AudioEngineHandle(Arc<Mutex<[Option<AudioEngine>; 2]>>);

unsafe impl Send for AudioEngineHandle {}
unsafe impl Sync for AudioEngineHandle {}

impl AudioEngineHandle {
    fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, [Option<AudioEngine>; 2]>> {
        self.0.lock()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub bus_number: u8,
    pub device_address: u8,
    pub product_string: Option<String>,
    pub serial_number: Option<String>,
}

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

#[tauri::command]
async fn load_library(usb_root: String, state: tauri::State<'_, AppState>) -> Result<Vec<Track>, String> {
    eprintln!("📚 [LOAD_LIBRARY] Iniciando - USB: {}", usb_root);

    let usb_path = Path::new(&usb_root);
    let mut tracks = read_library(usb_path)
        .map_err(|e| format!("Erro ao ler biblioteca Rekordbox: {}", e))?;

    // ✅ CORREÇÃO: Salvamos o tamanho ANTES do loop mutável
    let total_tracks = tracks.len();
    eprintln!("📚 [LOAD_LIBRARY] {} tracks lidas do XML", total_tracks);

    let cache = state.analysis_cache.clone();
    let mut cached_count = 0;
    let mut analyzed_count = 0;
    let mut error_count = 0;

    for (i, track) in tracks.iter_mut().enumerate() {
        // ✅ CORREÇÃO: Usamos a variável total_tracks aqui
        eprintln!("  [TRACK {}/{}] {} - {}", i + 1, total_tracks, track.title, track.file_path);

        let audio_path = Path::new(&track.file_path);
        if !audio_path.exists() {
            eprintln!("    ⚠️ Arquivo não existe no caminho especificado.");
            continue;
        }

        // 1. Tentar usar o cache
        // if let Some(cached) = cache.get(audio_path) {
        //    eprintln!("    ✅ Cache hit - waveform: {} pixels", cached.waveform_amplitudes.len());
        //    track.analysis = Some(cached);
        //    cached_count += 1;
        //    continue;
        //}

        // 2. Analisar o arquivo
        eprintln!("    🔬 Analisando áudio...");
        match analyze_audio(audio_path) {
            Ok(analysis) => {
                eprintln!(
                    "    ✅ Análise concluída - waveform: {} pixels, BPM: {:?}",
                    analysis.waveform_amplitudes.len(),
                    analysis.estimated_bpm
                );
                let _ = cache.insert(audio_path, analysis.clone());
                track.analysis = Some(analysis);
                analyzed_count += 1;
            }
            Err(e) => {
                eprintln!("    ❌ Erro na análise: {}", e);
                error_count += 1;
            }
        }
    }

    eprintln!(
        "📊 [LOAD_LIBRARY] Resumo: {} em cache, {} analisadas com sucesso, {} erros",
        cached_count, analyzed_count, error_count
    );

    // Atualiza o estado da biblioteca
    {
        let mut lib = state.library.lock().map_err(|e| e.to_string())?;
        *lib = tracks.clone();
    }

    // Salva último USB root usado
    {
        let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
        set_last_usb_root(&mut us, usb_root.clone());
        let us_clone = us.clone();
        drop(us);
        let _ = save_user_state(&us_clone);
    }

    eprintln!("✅ [LOAD_LIBRARY] Concluído - retornando {} tracks", total_tracks);
    Ok(tracks)
}

#[tauri::command]
async fn load_track_analysis(file_path: String) -> Result<opendeck_core::TrackAnalysis, String> {
    analyze_audio(Path::new(&file_path)).map_err(|e| format!("Erro ao analisar arquivo de áudio: {}", e))
}

#[tauri::command]
async fn get_cues_for_track(_track_id: String) -> Result<Vec<CuePoint>, String> {
    Ok(vec![])
}

#[tauri::command]
async fn play_track(file_path: String, deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    eprintln!("🎵 [PLAY_TRACK] Solicitado - file_path: {}, deck: {}", file_path, deck_index);
    
    if deck_index >= 2 {
        return Err("Índice de deck inválido (use 0 ou 1)".to_string());
    }

    let resolved = resolve_audio_path(&file_path);
    let final_path = resolved.as_deref().unwrap_or(&file_path);
    eprintln!("📁 [PLAY_TRACK] Caminho resolvido: {}", final_path);

    if !std::path::Path::new(final_path).exists() {
        eprintln!("❌ [PLAY_TRACK] Arquivo final não existe!");
        return Err(format!("Arquivo de áudio não encontrado: {}", final_path));
    }

    let mut engines = state.audio_engines.lock().map_err(|e| e.to_string())?;
    let engine = engines.get_mut(deck_index).ok_or("Deck não encontrado")?;

    if engine.is_none() {
        eprintln!("🔧 [PLAY_TRACK] Criando novo AudioEngine...");
        let mut new_engine = AudioEngine::new().map_err(|e| format!("Erro ao criar motor de áudio: {}", e))?;
        new_engine.load_track(final_path).map_err(|e| format!("Erro ao carregar track: {}", e))?;
        new_engine.start_stream().map_err(|e| format!("Erro ao iniciar stream: {}", e))?;
        new_engine.play().map_err(|e| format!("Erro ao iniciar reprodução: {}", e))?;
        *engine = Some(new_engine);
        eprintln!("✅ [PLAY_TRACK] Reprodução iniciada com sucesso (novo engine).");
    } else {
        eprintln!("🔄 [PLAY_TRACK] Reutilizando engine existente...");
        let engine_ref = engine.as_mut().unwrap();
        engine_ref.load_track(final_path).map_err(|e| format!("Erro ao carregar track: {}", e))?;
        if engine_ref.get_state() == PlaybackState::Stopped {
            engine_ref.start_stream().map_err(|e| format!("Erro ao iniciar stream: {}", e))?;
        }
        engine_ref.play().map_err(|e| format!("Erro ao iniciar reprodução: {}", e))?;
        eprintln!("✅ [PLAY_TRACK] Reprodução iniciada com sucesso (engine existente).");
    }
    Ok(())
}

#[tauri::command]
async fn pause_track(deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(engines) = state.audio_engines.lock() {
        if let Some(ref engine) = engines[deck_index] { engine.pause(); }
    }
    Ok(())
}

#[tauri::command]
async fn resume_track(deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(engines) = state.audio_engines.lock() {
        if let Some(ref engine) = engines[deck_index] {
            engine.play().map_err(|e| format!("Erro ao resumir: {}", e))?;
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado para este deck".to_string())
}

#[tauri::command]
async fn stop_track(deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] { engine.stop(); }
    }
    Ok(())
}

#[tauri::command]
async fn seek_track(deck_index: usize, position_ms: u64, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.seek(position_ms).map_err(|e| format!("Erro no seek: {}", e))?;
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado para este deck".to_string())
}

#[tauri::command]
async fn set_tempo(factor: f32, deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.set_tempo(factor);
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado".to_string())
}

#[tauri::command]
async fn set_pitch(semitones: f32, deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.set_pitch(semitones);
            return Ok(());
        }
    }
    Err("Motor de áudio não iniciado".to_string())
}

#[tauri::command]
async fn set_volume(volume: f32, deck_index: usize, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if deck_index >= 2 { return Err("Índice inválido".into()); }
    if let Ok(mut engines) = state.audio_engines.lock() {
        if let Some(ref mut engine) = engines[deck_index] {
            engine.set_volume(volume);
            return Ok(());
        }
    }
    Ok(())
}

fn resolve_audio_path(rel_path: &str) -> Option<String> {
    let p = std::path::Path::new(rel_path);
    if p.is_absolute() && p.exists() { return Some(rel_path.to_string()); }
    
    let file_name = p.file_name()?.to_string_lossy().to_string();
    let search_dirs = [
        r"D:\PIONEER\rekordbox", r"C:\Users\alsvi\Music", r"C:\Users\alsvi\Músicas",
        r"D:\Músicas", r"C:\Users\alsvi\Desktop", r"C:\Users\alsvi\Documents",
        r"C:\Users\alsvi\AppData\Roaming\Pioneer\rekordbox",
    ];
    
    for dir in &search_dirs {
        let full = std::path::Path::new(dir).join(&file_name);
        if full.exists() { return Some(full.to_string_lossy().to_string()); }
    }
    
    for dir in &search_dirs {
        let dir_path = std::path::Path::new(dir);
        if !dir_path.exists() { continue; }
        if let Some(entry) = walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) 
            .find(|e| e.file_name() == std::ffi::OsStr::new(&file_name)) {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

#[tauri::command]
async fn get_devices() -> Result<Vec<DeviceInfo>, String> {
    let scanner = UsbScanner::new().map_err(|e| format!("Erro ao criar scanner USB: {}", e))?;
    let devices = scanner.scan().map_err(|e| format!("Erro ao escanear dispositivos: {}", e))?;
    Ok(devices.into_iter().map(|d| DeviceInfo {
        vid: d.vid, pid: d.pid.as_u16(), bus_number: d.bus_number,
        device_address: d.device_address, product_string: d.product_string,
        serial_number: d.serial_number,
    }).collect())
}

#[tauri::command]
async fn get_user_state(state: tauri::State<'_, AppState>) -> Result<UserState, String> {
    let user_state = state.user_state.lock().map_err(|e| e.to_string())?;
    Ok(user_state.clone())
}

#[tauri::command]
async fn save_user_state_cmd(user_state: UserState, state: tauri::State<'_, AppState>) -> Result<(), String> {
    { let mut us = state.user_state.lock().map_err(|e| e.to_string())?; *us = user_state.clone(); }
    save_user_state(&user_state).map_err(|e| format!("Erro ao salvar estado: {}", e))
}

#[tauri::command]
async fn update_preferences(preferences: UserPreferences, state: tauri::State<'_, AppState>) -> Result<(), String> {
    { let mut us = state.user_state.lock().map_err(|e| e.to_string())?; us.preferences = preferences.clone(); }
    let us = { let guard = state.user_state.lock().map_err(|e| e.to_string())?; guard.clone() };
    save_user_state(&us).map_err(|e| format!("Erro ao salvar preferências: {}", e))
}

#[tauri::command]
async fn update_window_state_cmd(width: u32, height: u32, x: i32, y: i32, state: tauri::State<'_, AppState>) -> Result<(), String> {
    { let mut us = state.user_state.lock().map_err(|e| e.to_string())?;
      us.window_width = Some(width); us.window_height = Some(height);
      us.window_x = Some(x); us.window_y = Some(y); }
    let us = { let guard = state.user_state.lock().map_err(|e| e.to_string())?; guard.clone() };
    save_user_state(&us).map_err(|e| format!("Erro ao salvar estado da janela: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_library, load_track_analysis, get_cues_for_track,
            play_track, pause_track, resume_track, seek_track, stop_track,
            set_tempo, set_pitch, set_volume,
            get_devices, get_user_state, save_user_state_cmd,
            update_preferences, update_window_state_cmd,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info).build())?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}