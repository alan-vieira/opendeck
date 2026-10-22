//! Parser legado de arquivos `.DAT` (formato binário proprietário do Rekordbox).
//!
//! Mantido para retrocompatibilidade durante a transição da Fase 2.
//! Na Fase 3, este módulo será removido em favor da análise própria de áudio.

use byteorder::{BigEndian, ReadBytesExt};
use memmap2::Mmap;
use opendeck_core::TrackAnalysis;
use std::fs::File;
use std::io::Cursor;
use thiserror::Error;

/// Erros possíveis ao operar o parser de `.DAT`.
#[derive(Error, Debug)]
pub enum DatError {
    /// byteorder retorna std::io::Error, então usamos isso diretamente
    #[error("I/O or Byte Order error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File is too small to contain a valid header")]
    FileTooSmall,

    #[error("Invalid or corrupted chunk data")]
    InvalidChunk,
}

/// Parser de arquivos `.DAT` do Rekordbox usando zero-copy (memmap2).
pub struct DatParser;

impl DatParser {
    /// Lê o arquivo `.DAT` do disco usando zero-copy (memmap2)
    pub fn parse_file(file_path: &str) -> Result<TrackAnalysis, DatError> {
        let file = File::open(file_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Self::parse_bytes(&mmap)
    }

    /// Parseia um slice de bytes (útil para testes e para quando os dados já estão em memória)
    pub fn parse_bytes(data: &[u8]) -> Result<TrackAnalysis, DatError> {
        if data.len() < 8 {
            return Err(DatError::FileTooSmall);
        }

        let mut beatgrid_seconds = Vec::new();
        let mut waveform_amplitudes = Vec::new();

        let mut offset = 0;
        let len = data.len();
        let signatures = [b"PMAI", b"PVBR", b"PQTZ", b"PWAV", b"PCUE", b"PPTH"];

        while offset + 8 <= len {
            let mut found_sig = None;
            let mut next_offset = offset;

            // Lógica de Ressincronização: procura a próxima assinatura válida
            for &sig in &signatures {
                if let Some(pos) = data[offset..].windows(4).position(|w| w == sig) {
                    found_sig = Some(sig);
                    next_offset = offset + pos;
                    break;
                }
            }

            if let Some(sig) = found_sig {
                offset = next_offset;

                // Lê o tamanho do payload (Big-Endian u32)
                let mut cursor = Cursor::new(&data[offset + 4..offset + 8]);
                let payload_size = cursor.read_u32::<BigEndian>()? as usize;

                let payload_start = offset + 8;
                let payload_end = payload_start + payload_size;

                if payload_end > len {
                    break; // Evita pânico se o arquivo estiver truncado
                }

                let payload = &data[payload_start..payload_end];
                let _tag_str = String::from_utf8_lossy(sig);

                match _tag_str.as_ref() {
                    "PQTZ" => {
                        let mut pqtz_cursor = Cursor::new(payload);
                        while pqtz_cursor.position() as usize + 8 <= payload.len() {
                            let ms = pqtz_cursor.read_u32::<BigEndian>()?;
                            let _beat_type = pqtz_cursor.read_u32::<BigEndian>()?;

                            // Filtra valores absurdos (proteção contra lixo no arquivo)
                            if ms < 10_000_000 {
                                beatgrid_seconds.push(ms as f32 / 1000.0);
                            }
                        }
                    }
                    "PWAV" => {
                        waveform_amplitudes.extend_from_slice(payload);
                    }
                    _ => {
                        // PMAI, PVBR, etc. Podem ser implementados futuramente
                    }
                }

                // Avançar o offset e aplicar padding de 4 bytes
                offset = payload_end;
                if offset % 4 != 0 {
                    offset += 4 - (offset % 4);
                }
            } else {
                break; // Não encontrou mais assinaturas válidas
            }
        }

        Ok(TrackAnalysis {
            beatgrid_seconds,
            waveform_amplitudes,
            estimated_bpm: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_dat() {
        // Constrói um arquivo DAT falso com PQTZ e PWAV
        let mut pqtz_payload = Vec::new();
        pqtz_payload.extend_from_slice(&0x000011D0u32.to_be_bytes()); // 4560 ms
        pqtz_payload.extend_from_slice(&0x00000000u32.to_be_bytes()); // type 0
        pqtz_payload.extend_from_slice(&0x00001388u32.to_be_bytes()); // 5000 ms
        pqtz_payload.extend_from_slice(&0x00000000u32.to_be_bytes()); // type 0

        let mut pqtz_chunk = Vec::new();
        pqtz_chunk.extend_from_slice(b"PQTZ");
        pqtz_chunk.extend_from_slice(&(pqtz_payload.len() as u32).to_be_bytes());
        pqtz_chunk.extend_from_slice(&pqtz_payload);

        let pwav_payload = vec![0u8, 128, 255, 128];
        let mut pwav_chunk = Vec::new();
        pwav_chunk.extend_from_slice(b"PWAV");
        pwav_chunk.extend_from_slice(&(pwav_payload.len() as u32).to_be_bytes());
        pwav_chunk.extend_from_slice(&pwav_payload);

        let mut dat_bytes = Vec::new();
        dat_bytes.extend_from_slice(&pqtz_chunk);
        dat_bytes.extend_from_slice(&pwav_chunk);

        let result = DatParser::parse_bytes(&dat_bytes).unwrap();

        assert_eq!(result.beatgrid_seconds.len(), 2);
        assert!((result.beatgrid_seconds[0] - 4.560).abs() < 0.001);
        assert!((result.beatgrid_seconds[1] - 5.000).abs() < 0.001);

        assert_eq!(result.waveform_amplitudes, vec![0, 128, 255, 128]);
    }

    #[test]
    fn test_resync_after_corruption() {
        // Dados lixo + PQTZ válido
        let mut dat_bytes = vec![0u8; 100]; // 100 bytes de lixo
        dat_bytes.extend_from_slice(b"PQTZ");
        dat_bytes.extend_from_slice(&8u32.to_be_bytes()); // size = 8
        dat_bytes.extend_from_slice(&0x000011D0u32.to_be_bytes()); // 4560 ms
        dat_bytes.extend_from_slice(&0x00000000u32.to_be_bytes()); // type 0

        let result = DatParser::parse_bytes(&dat_bytes).unwrap();
        assert_eq!(result.beatgrid_seconds.len(), 1);
        assert!((result.beatgrid_seconds[0] - 4.560).abs() < 0.001);
    }
}
