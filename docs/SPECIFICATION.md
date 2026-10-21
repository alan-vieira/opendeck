# OpenDeck - Especificação Técnica de Dados

## 1. Formatos Decodificados (Knowledge Base)

### Arquivo: `track_analysis.DAT`
Estrutura de chunks TLV (Tag-Length-Value) com padding de 4 bytes.
- **Chunk `PQTZ` (Beatgrid)**: Array de `u32` Big-Endian representando milissegundos.
  - *Fórmula*: `segundos = ms / 1000.0`
- **Chunk `PWAV` (Waveform)**: Array contíguo de `u8` (0-255) representando amplitudes brutas.
  - *Renderização*: `pixel_height = (amplitude / 255) * canvas_height`

### Banco de Dados: `master.db`
- **Engine**: SQLite com extensão SQLCipher (Criptografia AES-256-CBC).
- **Tabelas Chave**:
  - `djmdContent`: Metadados principais da faixa (título, artista, caminho do arquivo, BPM).
  - `djmdCue`: Pontos de cue e loops salvos.
  - `djmdPlaylist`: Estrutura de pastas e playlists da biblioteca.
- **Nota de Segurança**: A chave de descriptografia do `master.db` é derivada de um identificador único do hardware ou configuração do usuário. O `opendeck-db` requer que essa chave seja fornecida via variável de ambiente ou prompt seguro na primeira execução.

## 2. Modelos de Dados (Rust)
```rust
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub bpm: Option<f32>,
    pub file_path: String,
    pub analysis_path: Option<String>,
}

pub struct TrackAnalysis {
    pub beatgrid_seconds: Vec<f32>,
    pub waveform_amplitudes: Vec<u8>,
}

pub struct CuePoint {
    pub id: String,
    pub track_id: String,
    pub time_ms: u32,
    pub cue_type: u8, // 0 = Memory, 1 = Loop, etc.
}

## 3. Requisitos de Áudio

- **Latência Alvo**: ≤ 5ms (buffer de 128 a 256 frames a 48kHz).
- **Formatos Suportados**: MP3, AAC, ALAC, FLAC, WAV (via `symphonia`).
- **Thread de Áudio**: Deve rodar em prioridade elevada (Real-time se possível no SO) para evitar *xruns* (dropouts).
```