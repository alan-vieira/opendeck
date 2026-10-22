//! Exemplo audível: toca um arquivo MP3 via `opendeck-audio`.
//!
//! Uso:
//!   cargo run --example play_mp3 -- "C:\Users\alsvi\Music\teste.mp3"
//!
//! Valida: inicialização, decodificação, reprodução, pause/resume, troca de faixa e shutdown limpo.

use opendeck_audio::AudioEngine;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Uso: cargo run --example play_mp3 -- <caminho-do-arquivo.mp3>");
        std::process::exit(1);
    }

    let audio_path = PathBuf::from(&args[1]);
    if !audio_path.exists() {
        eprintln!("Arquivo não encontrado: {}", audio_path.display());
        std::process::exit(1);
    }

    println!("🎧 OpenDeck - Teste Audível de Áudio");
    println!("   Arquivo: {}", audio_path.display());
    println!("   Inicializando motor...");

    let mut engine = match AudioEngine::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("❌ Falha ao criar AudioEngine: {}", e);
            std::process::exit(1);
        }
    };

    println!("   Dispositivo: {}", engine.device_name());
    println!("   Latência teórica: {:.2} ms", engine.theoretical_latency_ms());

    if let Err(e) = engine.start() {
        eprintln!("❌ Falha ao iniciar stream: {}", e);
        std::process::exit(1);
    }
    println!("   ▶️  Stream de áudio iniciado");

    println!("   ▶️  Carregando e tocando faixa...");
    if let Err(e) = engine.play_file(&audio_path) {
        eprintln!("❌ Falha ao tocar arquivo: {}", e);
        std::process::exit(1);
    }

    // Monitora buffer por 15 segundos enquanto toca
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let fill = engine.buffer_fill_seconds();
        let fill_pct = (fill / 2.0) * 100.0;
        let xruns = engine.get_xrun_count();
        print!(
            "\r   📊 Buffer: {:.1}s ({:.0}%) | Xruns: {} | Estado: {:?}",
            fill,
            fill_pct,
            xruns,
            engine.get_state()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    println!();

    println!("   ⏸️  Pausando...");
    engine.pause();
    std::thread::sleep(Duration::from_secs(2));

    println!("   ▶️  Retomando...");
    engine.play().unwrap();
    std::thread::sleep(Duration::from_secs(3));

    println!("   🔄 Trocando de faixa (mesmo arquivo)...");
    if let Err(e) = engine.play_file(&audio_path) {
        eprintln!("❌ Falha ao trocar de faixa: {}", e);
        std::process::exit(1);
    }
    std::thread::sleep(Duration::from_secs(3));

    println!("   ⏹️  Encerrando...");
    engine.shutdown();

    println!("   ✅ Teste audível concluído com sucesso!");
}
