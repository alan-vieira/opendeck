// crates/opendeck-audio/examples/play_test.rs
use opendeck_audio::{AudioEngine, AudioError};
use std::env;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn find_first_audio_file(dir: &str) -> Option<String> {
    let extensions = ["mp3", "wav", "flac", "aac"];
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext.to_lowercase().as_str()) {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

fn main() -> Result<(), AudioError> {
    println!("🎧 OpenDeck Audio Test - Motor de Áudio de Baixa Latência\n");
    
    // 1. Inicializa o motor de áudio
    println!("📡 Inicializando motor de áudio...");
    let mut engine = AudioEngine::new()?;
    
    println!("✅ Motor inicializado com sucesso!");
    println!("   Sample Rate: {} Hz", engine.sample_rate());
    println!("   Buffer Size: {} samples", engine.buffer_size());
    println!("   Latência Teórica: {:.2} ms\n", engine.theoretical_latency_ms());
    
    // 2. Descobre o arquivo de áudio
    let track_path = env::args().nth(1).unwrap_or_else(|| {
        find_first_audio_file(r"D:\Músicas\80").expect(
            "Nenhum arquivo .mp3, .wav ou .flac encontrado em D:\\Músicas.\n\
             Por favor, execute com: cargo run --example play_test -p opendeck-audio -- \"C:\\caminho\\para\\sua\\musica.mp3\""
        )
    });
    
    println!("📂 Carregando: {}", track_path);
    engine.load_track(&track_path)?;
    println!("✅ Track carregada com sucesso!\n");
    
    // 3. Inicia a reprodução
    println!("▶️  Iniciando reprodução... (Pressione Ctrl+C para parar)\n");
    engine.play();
    
    // 4. Monitora a reprodução por 15 segundos
    println!("Tempo (s) | XRUNs (Dropouts) | Status");
    println!("----------|------------------|--------");
    
    for i in 1..=15 {
        thread::sleep(Duration::from_secs(1));
        let xruns = engine.get_xrun_count();
        let status = if xruns == 0 { "✅ Perfeito" } else { "⚠️  DROPOUT" };
        println!("{:<9} | {:<16} | {}", i, xruns, status);
    }
    
    // 5. Para a reprodução
    println!("\n⏹️  Parando reprodução...");
    engine.stop();
    
    println!("\n🏁 Teste concluído!");
    println!("   Total de XRUNs: {}", engine.get_xrun_count());
    if engine.get_xrun_count() == 0 {
        println!("   🎉 Seu motor de áudio está funcionando com latência profissional! 🎉\n");
    } else {
        println!("   ⚠️  Alguns dropouts detectados. Isso pode ser normal no Windows sem otimizações de energia.\n");
    }
    
    Ok(())
}