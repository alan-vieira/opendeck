//! Leitor do banco de dados `master.db` do Rekordbox 6.x.
//!
//! Este crate encapsula a conexão `rusqlite` com SQLite bundulado
//! (sem dependência externa do OpenSSL) e expõe métodos tipados para
//! consultar tracks e cue points.

use std::path::Path;

use opendeck_core::{CuePoint, CueType, Track};
use rusqlite::{Connection, Error as RusqliteError, OptionalExtension, Row};
use thiserror::Error;

/// Erros possíveis ao operar no banco de dados Rekordbox.
#[derive(Error, Debug)]
pub enum DbError {
    #[error("Erro ao abrir conexão SQLite: {0}")]
    Connection(#[from] RusqliteError),

    #[error("Chave de criptografia inválida ou banco corrompido: {0}")]
    InvalidKey(String),

    #[error("Schema inválido: tabela esperada não encontrada: {0}")]
    SchemaMissing(String),

    #[error("Erro de conversão de dados: {0}")]
    Conversion(String),

    #[error("Track não encontrado: {0}")]
    TrackNotFound(String),
}

/// Wrapper tipado para `rusqlite::Connection`.
#[derive(Debug)]
pub struct RekordboxDb {
    conn: Connection,
}

impl RekordboxDb {
    /// Abre o banco `master.db` do Rekordbox e valida a chave SQLCipher.
    ///
    /// # Argumentos
    /// * `db_path` - Caminho para o arquivo `master.db`.
    /// * `key` - Chave de criptografia (senha) usada no `PRAGMA key`.
    ///
    /// # Retorna
    /// `Ok(RekordboxDb)` se a conexão abrir e a chave for válida.
    ///
    /// # Erros
    /// * `DbError::Connection` - Falha de I/O ou SQLite.
    /// * `DbError::InvalidKey` - Chave incorreta (falha no `PRAGMA key` ou schema inacessível).
    /// * `DbError::SchemaMissing` - Tabelas esperadas não existem.
    pub fn open<P: AsRef<Path>>(db_path: P, key: &str) -> Result<Self, DbError> {
        let conn = Connection::open(db_path)?;
       
        // Define a chave de criptografia SQLCipher. PRAGMA key do SQLCipher não
        // aceita parâmetros bind (?), então escapamos aspas simples na chave
        // para evitar injection antes de interpolar no execute_batch.
        let safe_key = key.replace('\'', "''");
        conn.execute_batch(&format!("PRAGMA key = '{}';", safe_key))?;
       
        // Valida a chave tentando ler o schema
        // Se a chave estiver errada, o PRAGMA key não falha imediatamente,
        // mas as queries subsequentes falharão com "file is encrypted or is not a database"
        let table_exists: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='djmdContent' LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
       
        if table_exists.is_none() {
            return Err(DbError::InvalidKey(
                "Tabela 'djmdContent' não encontrada. Chave incorreta ou banco corrompido.".to_string(),
            ));
        }
       
        // Valida tabelas essenciais
        Self::validate_schema(&conn)?;
       
        Ok(Self { conn })
    }

    /// Valida se as tabelas essenciais do schema Rekordbox existem.
    fn validate_schema(conn: &Connection) -> Result<(), DbError> {
        let required_tables = ["djmdContent", "djmdCue", "djmdPlaylist"];
        
        for table in required_tables {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
                    [table],
                    |row| row.get(0),
                )
                .optional()?;
            
            if exists.is_none() {
                return Err(DbError::SchemaMissing(format!(
                    "Tabela obrigatória '{}' não encontrada no schema",
                    table
                )));
            }
        }
        
        Ok(())
    }

    /// Retorna todas as tracks da tabela `djmdContent` com join no artista.
        ///
        /// Query: JOIN djmdContent + djmdArtist para obter nome do artista
        pub fn get_tracks(&self) -> Result<Vec<Track>, DbError> {
            let mut stmt = self.conn.prepare(
                "SELECT c.ID, c.Title, a.Name, c.BPM, c.rb_LocalFolderPath, c.FileNameL, c.AnalysisDataPath
                 FROM djmdContent c
                 LEFT JOIN djmdArtist a ON c.ArtistID = a.ID
                 WHERE c.rb_local_deleted = 0",
            )?;

            let track_iter = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let artist: Option<String> = row.get(2)?;
                let bpm: Option<i32> = row.get(3)?;
                let folder_path: Option<String> = row.get(4)?;
                let file_name: Option<String> = row.get(5)?;
                let analysis_path: Option<String> = row.get(6)?;

                let file_path = match (folder_path, file_name) {
                    (Some(fp), Some(fname)) if !fp.is_empty() && !fname.is_empty() => {
                        format!("{}/{}", fp.trim_end_matches('/').trim_end_matches('\\'), fname)
                    }
                    (Some(fp), _) if !fp.is_empty() => fp,
                    (_, Some(fname)) if !fname.is_empty() => fname,
                    _ => String::new(),
                };

                Ok(Track {
                    id,
                    title,
                    artist: artist.unwrap_or_default(),
                    bpm: bpm.map(|b| b as f32 / 100.0),
                    file_path,
                    analysis_path,
                })
            })?;
       
            let mut tracks = Vec::new();
            for track_result in track_iter {
                tracks.push(track_result?);
            }
       
            Ok(tracks)
        }

    /// Retorna os cue points de uma track específica.
    ///
    /// Query: `SELECT ID, ContentID, InMsec, Type FROM djmdCue WHERE ContentID = ?`
    /// Converte `InMsec` (milissegundos) para `time_seconds: f32`.
    pub fn get_cues_for_track(&self, track_id: &str) -> Result<Vec<CuePoint>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT ID, ContentID, InMsec, Type FROM djmdCue WHERE ContentID = ?1"
        )?;
        
        let cue_iter = stmt.query_map([track_id], |row| self.row_to_cue(row))?;
        
        let mut cues = Vec::new();
        for cue_result in cue_iter {
            cues.push(cue_result?);
        }
        
        Ok(cues)
    }

    /// Retorna uma track específica por ID.
    pub fn get_track_by_id(&self, track_id: &str) -> Result<Track, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT c.ID, c.Title, a.Name, c.BPM, c.rb_LocalFolderPath, c.FileNameL, c.AnalysisDataPath
             FROM djmdContent c
             LEFT JOIN djmdArtist a ON c.ArtistID = a.ID
             WHERE c.ID = ?1",
        )?;

        let track = stmt.query_row([track_id], |row| self.row_to_track(row))
            .optional()?
            .ok_or_else(|| DbError::TrackNotFound(track_id.to_string()))?;

        Ok(track)
    }

    /// Converte uma linha da tabela `djmdContent` para `Track`.
    fn row_to_track(&self, row: &Row) -> Result<Track, RusqliteError> {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let artist: Option<String> = row.get(2)?;
        let bpm: Option<i32> = row.get(3)?;
        let folder_path: Option<String> = row.get(4)?;
        let file_name: Option<String> = row.get(5)?;
        let analysis_path: Option<String> = row.get(6)?;

        let file_path = match (folder_path, file_name) {
            (Some(fp), Some(fname)) if !fp.is_empty() && !fname.is_empty() => {
                format!("{}/{}", fp.trim_end_matches('/').trim_end_matches('\\'), fname)
            }
            (Some(fp), _) if !fp.is_empty() => fp,
            (_, Some(fname)) if !fname.is_empty() => fname,
            _ => String::new(),
        };

        // O caminho do analysis pode ser derivado ou buscado separadamente
        // Por enquanto deixamos como None
        Ok(Track {
            id,
            title,
            artist: artist.unwrap_or_default(),
            bpm: bpm.map(|b| b as f32 / 100.0),
            file_path,
            analysis_path,
        })
    }

    /// Converte uma linha da tabela `djmdCue` para `CuePoint`.
    /// Converte `InMsec` (u32 milissegundos) para `time_seconds: f32`.
    fn row_to_cue(&self, row: &Row) -> Result<CuePoint, RusqliteError> {
        let id: String = row.get(0)?;
        let track_id: String = row.get(1)?;
        let in_msec: u32 = row.get(2)?;
        let cue_type_int: i32 = row.get(3)?;
        
        let time_seconds = in_msec as f32 / 1000.0;
        
        let cue_type = match cue_type_int {
            0 => CueType::Normal,
            1 => CueType::LoopIn,
            2 => CueType::LoopOut,
            3 => CueType::Memory,
            _ => CueType::Normal, // fallback seguro
        };
        
        Ok(CuePoint {
            id,
            track_id,
            time_seconds,
            cue_type,
            name: None, // djmdCue não tem nome direto no schema padrão
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Cria um banco de dados de teste com SQLCipher e schema mínimo do Rekordbox.
    fn create_test_db() -> (NamedTempFile, String) {
        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");
        let db_path = temp_file.path().to_path_buf();
        let key = "test_key_123";

        // Cria o banco com SQLCipher
        let conn = Connection::open(&db_path).expect("Falha ao abrir conexão de teste");
        let safe_key = key.replace('\'', "''");
        conn.execute_batch(&format!("PRAGMA key = '{}';", safe_key)).expect("Falha no PRAGMA key");

        // Cria schema mínimo
        conn.execute_batch(
            "CREATE TABLE djmdContent (
                ID TEXT PRIMARY KEY,
                Title TEXT NOT NULL,
                Artist TEXT NOT NULL,
                ArtistID INTEGER,
                BPM INTEGER,
                rb_LocalFolderPath TEXT,
                FileNameL TEXT,
                AnalysisDataPath TEXT,
                rb_local_deleted INTEGER DEFAULT 0,
                FilePath TEXT
            );
            CREATE TABLE djmdCue (
                ID TEXT PRIMARY KEY,
                ContentID TEXT NOT NULL,
                InMsec INTEGER NOT NULL,
                Type INTEGER NOT NULL,
                FOREIGN KEY(ContentID) REFERENCES djmdContent(ID)
            );
            CREATE TABLE djmdArtist (
                ID INTEGER PRIMARY KEY,
                Name TEXT NOT NULL
            );
            CREATE TABLE djmdPlaylist (
                ID TEXT PRIMARY KEY,
                Name TEXT NOT NULL
            );",
        )
        .expect("Falha ao criar schema de teste");

        // Insere dados de teste
        conn.execute(
            "INSERT INTO djmdContent (ID, Title, Artist, BPM, rb_LocalFolderPath, FileNameL) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ("track_001", "Test Track 1", "Artist A", 12800i32, "/music", "track1.mp3"),
        )
        .expect("Falha ao inserir track 1");

        conn.execute(
            "INSERT INTO djmdContent (ID, Title, Artist, BPM, rb_LocalFolderPath, FileNameL) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ("track_002", "Test Track 2", "Artist B", 14050i32, "/music", "track2.mp3"),
        )
        .expect("Falha ao inserir track 2");

        conn.execute(
            "INSERT INTO djmdCue (ID, ContentID, InMsec, Type) VALUES (?1, ?2, ?3, ?4)",
            ("cue_001", "track_001", 4560u32, 0i32),
        )
        .expect("Falha ao inserir cue 1");

        conn.execute(
            "INSERT INTO djmdCue (ID, ContentID, InMsec, Type) VALUES (?1, ?2, ?3, ?4)",
            ("cue_002", "track_001", 9120u32, 1i32),
        )
        .expect("Falha ao inserir cue 2");

        conn.execute(
            "INSERT INTO djmdCue (ID, ContentID, InMsec, Type) VALUES (?1, ?2, ?3, ?4)",
            ("cue_003", "track_002", 13680u32, 3i32),
        )
        .expect("Falha ao inserir cue 3");

        conn.execute(
            "INSERT INTO djmdArtist (ID, Name) VALUES (?1, ?2)",
            (1i32, "Artist A"),
        )
        .expect("Falha ao inserir artista 1");

        conn.execute(
            "INSERT INTO djmdArtist (ID, Name) VALUES (?1, ?2)",
            (2i32, "Artist B"),
        )
        .expect("Falha ao inserir artista 2");

        conn.execute("UPDATE djmdContent SET ArtistID = 1 WHERE ID = 'track_001'", [])
            .expect("Falha ao atualizar ArtistID track 1");

        conn.execute("UPDATE djmdContent SET ArtistID = 2 WHERE ID = 'track_002'", [])
            .expect("Falha ao atualizar ArtistID track 2");

        drop(conn);
        (temp_file, key.to_string())
    }

    #[test]
    fn test_open_valid_database() {
        let (_temp_file, key) = create_test_db();
        let db_path = _temp_file.path();

        let db = RekordboxDb::open(db_path, &key).expect("Falha ao abrir banco válido");

        // Verifica se a conexão funciona
        let tracks = db.get_tracks().expect("Falha ao buscar tracks");
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn test_open_corrupted_database() {
        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");
        let db_path = temp_file.path();

        // Cria banco sem a tabela djmdContent (schema corrompido)
        let conn = Connection::open(db_path).expect("Falha ao abrir conexão");
        conn.execute_batch("CREATE TABLE other_table (id INTEGER); ").expect("Falha ao criar tabela ");
        drop(conn);

        let err = RekordboxDb::open(db_path, "test_key").unwrap_err();
        assert!(matches!(err, DbError::SchemaMissing(_) | DbError::Connection(_)));
    }

    #[test]
    fn test_get_tracks() {
        let (_temp_file, key) = create_test_db();
        let db = RekordboxDb::open(_temp_file.path(), &key).expect("Falha ao abrir banco");

        let tracks = db.get_tracks().expect("Falha ao buscar tracks");

        assert_eq!(tracks.len(), 2);

        let track1 = &tracks[0];
        assert_eq!(track1.id, "track_001");
        assert_eq!(track1.title, "Test Track 1");
        assert_eq!(track1.artist, "Artist A");
        assert!((track1.bpm.unwrap() - 128.0).abs() < f32::EPSILON);
        assert_eq!(track1.file_path, "/music/track1.mp3");
        assert!(track1.analysis_path.is_none());

        let track2 = &tracks[1];
        assert_eq!(track2.id, "track_002");
        assert_eq!(track2.artist, "Artist B");
    }

    #[test]
    fn test_get_cues_for_track() {
        let (_temp_file, key) = create_test_db();
        let db = RekordboxDb::open(_temp_file.path(), &key).expect("Falha ao abrir banco");

        // Track 1 tem 2 cues
        let cues = db.get_cues_for_track("track_001").expect("Falha ao buscar cues");

        assert_eq!(cues.len(), 2);

        let cue1 = &cues[0];
        assert_eq!(cue1.id, "cue_001");
        assert_eq!(cue1.track_id, "track_001");
        assert!((cue1.time_seconds - 4.560).abs() < f32::EPSILON);
        assert!(matches!(cue1.cue_type, CueType::Normal));

        let cue2 = &cues[1];
        assert_eq!(cue2.id, "cue_002");
        assert!((cue2.time_seconds - 9.120).abs() < f32::EPSILON);
        assert!(matches!(cue2.cue_type, CueType::LoopIn));

        // Track 2 tem 1 cue
        let cues2 = db.get_cues_for_track("track_002").expect("Falha ao buscar cues track 2");
        assert_eq!(cues2.len(), 1);
        assert!((cues2[0].time_seconds - 13.680).abs() < f32::EPSILON);
        assert!(matches!(cues2[0].cue_type, CueType::Memory));

        // Track inexistente retorna vetor vazio
        let empty = db.get_cues_for_track("nonexistent").expect("Falha ao buscar cues inexistente");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_get_track_by_id() {
        let (_temp_file, key) = create_test_db();
        let db = RekordboxDb::open(_temp_file.path(), &key).expect("Falha ao abrir banco");

        let track = db.get_track_by_id("track_001").expect("Falha ao buscar track por ID");
        assert_eq!(track.id, "track_001");
        assert_eq!(track.title, "Test Track 1");

        let err = db.get_track_by_id("nonexistent").unwrap_err();
        assert!(matches!(err, DbError::TrackNotFound(_)));
    }

    #[test]
    fn test_schema_validation_missing_table() {
        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");

        // Cria banco sem a tabela djmdContent
        let conn = Connection::open(temp_file.path()).expect("Falha ao abrir conexão");
        conn.execute_batch("CREATE TABLE other_table (id INTEGER); ").expect("Falha ao criar tabela ");
        drop(conn);

        let err = RekordboxDb::open(temp_file.path(), "test_key").unwrap_err();
        assert!(matches!(err, DbError::SchemaMissing(_) | DbError::Connection(_)));
    }

    #[test]
    fn test_msec_to_seconds_conversion() {
        // Testa conversão exata: 4560ms -> 4.560s
        let test_cases = [
            (0u32, 0.0f32),
            (1000, 1.0),
            (4560, 4.560),
            (9120, 9.120),
            (13680, 13.680),
            (123456, 123.456),
        ];
        
        for (msec, expected_sec) in test_cases {
            let seconds = msec as f32 / 1000.0;
            assert!((seconds - expected_sec).abs() < f32::EPSILON,
                "Falha para {}ms: esperado {}s, obtido {}s ", msec, expected_sec, seconds);
        }
    }
}