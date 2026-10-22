//! Teste de estresse do motor de áudio
//! Valida que o buffer não causa underruns sob carga de CPU e troca rápida de faixas.

use opendeck_audio::AudioEngine;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn get_test_audio() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/test_track.mp3");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[test]
fn test_audio_stress_30_seconds() {
    let Some(audio_path) = get_test_audio() else {
        println!("⚠️  Coloque um MP3 em tests/fixtures/test_track.mp3 para rodar o teste de estresse.");
        println!("   Pulando.");
        return;
    };

    println!("\n🔥 Iniciando teste de estresse (30 segundos)...");

    let mut engine = AudioEngine::new().expect("Falha ao inicializar AudioEngine");
    engine.start().expect("Falha ao iniciar stream de áudio");

    let start = Instant::now();
    engine.play_file(&audio_path).expect("Falha ao tocar arquivo");

    // Carga de CPU paralela para simular sistema sob estresse
    let stress_handle = std::thread::spawn(|| {
        let mut counter: u64 = 0;
        for _ in 0..10_000_000 {
            counter = counter.wrapping_add(1);
            let _ = (counter as f64).sqrt().sin().cos();
        }
    });

    // Monitora por 30 segundos
    let mut min_buffer = f32::INFINITY;
    let mut max_xruns = 0u64;
    while start.elapsed() < Duration::from_secs(30) {
        let fill = engine.buffer_fill_seconds();
        min_buffer = min_buffer.min(fill);
        max_xruns = max_xruns.max(engine.get_xrun_count());
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = stress_handle.join();
    engine.shutdown();

    println!("   Buffer mínimo observado: {:.2}s", min_buffer);
    println!("   Xruns máximos: {}", max_xruns);

    if min_buffer < 0.5 {
        panic!("❌ Buffer abaixo de 0.5s - risco de underrun");
    }
    println!("✅ Teste de estresse concluído sem falhas");
}

#[test]
fn test_rapid_track_switching() {
    let Some(audio_path) = get_test_audio() else {
        println!("⚠️  Arquivo de teste não encontrado. Pulando teste de troca rápida.");
        return;
    };

    println!("\n🔄 Testando troca rápida de faixas...");

    let mut engine = AudioEngine::new().expect("Falha ao inicializar AudioEngine");
    engine.start().expect("Falha ao iniciar stream de áudio");

    // Troca de faixa 10 vezes rapidamente (simula DJ trocando música)
    for i in 0..10 {
        println!("   Troca {}...", i + 1);
        engine.play_file(&audio_path).expect("Falha ao tocar na troca");
        std::thread::sleep(Duration::from_millis(200));
    }

    std::thread::sleep(Duration::from_secs(2));
    engine.shutdown();

    println!("✅ Troca rápida de faixas funcionou sem crash");
}

#[test]
fn test_pause_resume_stability() {
    let Some(audio_path) = get_test_audio() else {
        println!("⚠️  Arquivo de teste não encontrado. Pulando teste de pause/resume.");
        return;
    };

    println!("\n⏸️  Testando estabilidade de pause/resume...");

    let mut engine = AudioEngine::new().expect("Falha ao inicializar AudioEngine");
    engine.start().expect("Falha ao iniciar stream de áudio");
    engine.play_file(&audio_path).expect("Falha ao tocar");

    // Toca um pouco, pausa, retoma, para
    std::thread::sleep(Duration::from_secs(2));
    engine.pause();
    assert_eq!(engine.get_state(), opendeck_audio::PlaybackState::Paused);
    std::thread::sleep(Duration::from_secs(2));

    engine.play().expect("Falha ao retomar");
    assert_eq!(engine.get_state(), opendeck_audio::PlaybackState::Playing);
    std::thread::sleep(Duration::from_secs(2));

    engine.stop();
    assert_eq!(engine.get_state(), opendeck_audio::PlaybackState::Stopped);

    engine.shutdown();
    println!("✅ Pause/Resume/Stop estáveis");
}
