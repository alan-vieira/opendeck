use opendeck_db::RekordboxDb;

fn main() {
    let db_path = r"C:\Users\alsvi\AppData\Roaming\Pioneer\rekordbox\master.db";
    let key = "402fd482c38817c35ffa8ffb8c7d93143b749e7d315df7a81732a1ff43608497";
    println!("Tentando abrir: {}", db_path);

    match RekordboxDb::open(db_path, key) {
        Ok(db) => {
            match db.get_tracks() {
                Ok(tracks) => {
                    println!("Sucesso! {} tracks carregadas", tracks.len());
                    for t in tracks.iter().take(5) {
                        println!(
                            "  {} - {} ({}) - BPM: {:?} - {}",
                            t.id, t.title, t.artist, t.bpm, t.file_path
                        );
                    }
                }
                Err(e) => println!("Erro ao buscar tracks: {}", e),
            }
        }
        Err(e) => println!("Erro ao abrir banco: {}", e),
    }
}
