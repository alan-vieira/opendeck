# OpenDeck - Especificação Técnica de Dados

## 1. Formatos Decodificados (Knowledge Base)

### Arquivo: `track_analysis.DAT`
Estrutura de chunks TLV (Tag-Length-Value) com padding de 4 bytes.
- **Chunk `PQTZ` (Beatgrid)**: Array de `u32` Big-Endian representando milissegundos.
  - *Fórmula*: `segundos = ms / 1000.0`
- **Chunk `PWAV` (Waveform)**: Array contíguo de `u8` (0-255) representando amplitudes brutas.
  - *Renderização*: `pixel_height = (amplitude / 255) * canvas_height`

### Banco de Dados: `master.db`
- **Engine**: SQLite com SQLCipher (AES-256).
- **Tabelas Chave**: `djmdContent` (metadados), `djmdCue` (pontos de cue), `djmdPlaylist`.

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
```