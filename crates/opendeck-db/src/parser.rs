//! Parser de `rekordbox.xml` da biblioteca exportada pelo Rekordbox.
//!
//! Lê os metadados das tracks (Track, BPM, caminho do arquivo) contidos na
//! seção `<COLLECTION>` do XML. A análise detalhada (beatgrid/waveform)
//! continua sendo lida dos arquivos `.DAT` via `opendeck-analysis`.

use crate::error::XmlParseError;
use opendeck_core::Track;
use roxmltree::Document;
use std::path::Path;

/// Estrutura bruta extraída de um nó `<TRACK>` do XML.
#[derive(Debug, Clone)]
struct RawTrack {
    track_id: String,
    title: String,
    artist: String,
    bpm: Option<f32>,
    location: Option<String>,
    analysis_path: Option<String>,
    cues: Vec<RawCue>,
}

/// Estrutura bruta extraída de um nó `<POSITION_MARK>` do XML.
#[derive(Debug, Clone)]
struct RawCue {
    name: String,
    cue_type: u8,
    start_ms: u32,
    end_ms: Option<u32>,
    num: Option<u8>,
    color: Option<String>,
}

/// Parser do formato `rekordbox.xml`.
pub struct RekordboxXmlParser;

impl RekordboxXmlParser {
    /// Parseia um arquivo `rekordbox.xml` e retorna todas as tracks da coleção.
    pub fn parse_tracks(xml_path: &Path) -> Result<Vec<Track>, XmlParseError> {
        let xml_content = std::fs::read_to_string(xml_path)?;
        let document = Document::parse(&xml_content)?;

        let mut tracks = Vec::new();

        // Percorre apenas <TRACK> que pertencem à coleção (têm atributo Name).
        // Nós <TRACK> dentro de <PLAYLISTS> (apenas com Key) são ignorados.
        for track_node in document
            .descendants()
            .filter(|n| n.has_tag_name("TRACK") && n.attribute("Name").is_some())
        {
            let raw = Self::extract_track(track_node)?;
            if let Some(track) = Self::raw_to_track(raw) {
                tracks.push(track);
            }
        }

        Ok(tracks)
    }

    /// Extrai os atributos brutos de um nó `<TRACK>`.
    fn extract_track(node: roxmltree::Node) -> Result<RawTrack, XmlParseError> {
        let track_id = node
            .attribute("TrackID")
            .or_else(|| node.attribute("Key"))
            .ok_or_else(|| XmlParseError::MissingAttribute("TrackID".to_string()))?
            .to_string();

        let title = node
            .attribute("Name")
            .ok_or_else(|| XmlParseError::MissingAttribute("Name".to_string()))?
            .to_string();

        let artist = node
            .attribute("Artist")
            .map(|s| s.to_string())
            .unwrap_or_default();

        let bpm = node
            .attribute("BPM")
            .or_else(|| node.attribute("AverageBpm"))
            .and_then(|s| s.parse::<f32>().ok());

        // O Rekordbox guarda o caminho em Location / rb_Location / FilePath
        // pode ser um URL file://localhost/... ou um caminho direto.
        let location = node
            .attribute("Location")
            .or_else(|| node.attribute("rb_Location"))
            .or_else(|| node.attribute("FilePath"))
            .map(|s| s.to_string());

        let analysis_path = node
            .attribute("AnalysisDataPath")
            .map(|s| s.to_string());

        // Extrai cues dos nós <POSITION_MARK> filhos deste <TRACK>
        let mut cues = Vec::new();
        for cue_node in node.descendants().filter(|n| {
            n.has_tag_name("POSITION_MARK") && n.parent().map_or(false, |p| p == node)
        }) {
            let raw_cue = Self::extract_cue(cue_node)?;
            cues.push(raw_cue);
        }

        Ok(RawTrack {
            track_id,
            title,
            artist,
            bpm,
            location,
            analysis_path,
            cues,
        })
    }

    /// Extrai os atributos brutos de um nó `<POSITION_MARK>`.
    fn extract_cue(node: roxmltree::Node) -> Result<RawCue, XmlParseError> {
        let name = node
            .attribute("Name")
            .map(|s| s.to_string())
            .unwrap_or_default();

        let cue_type: u8 = node
            .attribute("Type")
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0);

        let start_ms: u32 = node
            .attribute("Start")
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or_else(|| XmlParseError::MissingAttribute("Start".to_string()))?;

        let end_ms: Option<u32> = node.attribute("End").and_then(|s| s.parse::<u32>().ok());

        let num: Option<u8> = node.attribute("Num").and_then(|s| s.parse::<u8>().ok());

        let color = node.attribute("Colour").map(|s| s.to_string());

        Ok(RawCue {
            name,
            cue_type,
            start_ms,
            end_ms,
            num,
            color,
        })
    }

    /// Converte os dados brutos em um `Track` do `opendeck-core`.
    fn raw_to_track(raw: RawTrack) -> Option<Track> {
        let file_path = raw.location.as_deref().map(Self::normalize_location).unwrap_or_default();

        let cues: Vec<opendeck_core::CuePoint> = raw
            .cues
            .into_iter()
            .enumerate()
            .map(|(index, cue)| opendeck_core::CuePoint {
                id: format!("{}_cue{}", raw.track_id, index),
                track_id: raw.track_id.clone(),
                name: cue.name,
                cue_type: Self::map_cue_type(cue.cue_type),
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                hot_cue_index: if cue.cue_type == 0 { cue.num } else { None },
                color_rgb: cue.color.and_then(|c| Self::parse_color(&c)),
            })
            .collect();

        Some(Track {
            id: raw.track_id,
            title: raw.title,
            artist: raw.artist,
            bpm: raw.bpm,
            file_path,
            analysis_path: raw.analysis_path,
            cues,
            analysis: None,
        })
    }

    /// Mapeia o tipo numérico do Rekordbox para `CueType`.
    /// Rekordbox tipos: 0=Memory/Hot Cue, 1=Loop, 3=Fade In, 4=Fade Out
    fn map_cue_type(type_code: u8) -> opendeck_core::CueType {
        match type_code {
            0 => opendeck_core::CueType::Memory,
            1 => opendeck_core::CueType::Loop,
            3 => opendeck_core::CueType::FadeIn,
            4 => opendeck_core::CueType::FadeOut,
            other => opendeck_core::CueType::Unknown(other),
        }
    }

    /// Converte uma string de cor hex (ex: "0xFF00FF") para (u8, u8, u8).
    fn parse_color(hex: &str) -> Option<(u8, u8, u8)> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        u32::from_str_radix(hex, 16).ok().map(|v| {
            (
                ((v >> 16) & 0xFF) as u8,
                ((v >> 8) & 0xFF) as u8,
                (v & 0xFF) as u8,
            )
        })
    }

    /// Normaliza um caminho de localização do Rekordbox para um path do sistema.
    ///
    /// O Rekordbox exporta caminhos como:
    /// - `file://localhost/C:/Música/track.mp3`
    /// - `file:///C:/Música/track.mp3`
    /// - `C:\Música\track.mp3` (já absoluto)
    pub fn normalize_location(location: &str) -> String {
        let trimmed = location.trim();

        // Remove o prefixo file:// ou file://localhost
        if let Some(rest) = trimmed.strip_prefix("file://localhost/") {
            return rest.to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("file:///") {
            return rest.to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return rest.to_string();
        }

        // Já é um caminho direto
        trimmed.to_string()
    }
}

/// Lê a biblioteca completa de um Pen Drive exportado pelo Rekordbox.
///
/// Espera o arquivo em `<usb_root>/PIONEER/rekordbox/rekordbox.xml`.
pub fn read_library(usb_root: &Path) -> Result<Vec<Track>, XmlParseError> {
    let xml_path = usb_root.join("PIONEER").join("rekordbox").join("rekordbox.xml");

    if !xml_path.exists() {
        return Err(XmlParseError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("rekordbox.xml not found in {:?}", xml_path),
        )));
    }

    RekordboxXmlParser::parse_tracks(&xml_path)
}

/// Lê apenas as faixas de uma playlist específica.
///
/// Implementação futura: parsear nós `<PLAYLIST>` no XML.
pub fn read_playlist(_usb_root: &Path, _playlist_name: &str) -> Result<Vec<Track>, XmlParseError> {
    Err(XmlParseError::UnsupportedFormat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    const SAMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="6.6.0" />
  <COLLECTION Entries="2">
    <TRACK TrackID="1" Name="Track One" Artist="Artist A" BPM="128.00"
      Location="file://localhost/C:/Musica/track1.mp3" />
    <TRACK TrackID="2" Name="Track Two" Artist="Artist B" BPM="140.50"
      Location="file://localhost/D:/Musica/track2.mp3" />
  </COLLECTION>
  <PLAYLISTS>
    <NODE Name="Root" Type="0" Count="1" />
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    fn write_sample_xml() -> NamedTempFile {
        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");
        std::fs::write(temp_file.path(), SAMPLE_XML).expect("Falha ao escrever XML de teste");
        temp_file
    }

    #[test]
    fn test_parse_tracks_from_xml() {
        let temp_file = write_sample_xml();
        let tracks = RekordboxXmlParser::parse_tracks(temp_file.path()).expect("Falha ao parsear XML");

        assert_eq!(tracks.len(), 2);

        let t1 = &tracks[0];
        assert_eq!(t1.id, "1");
        assert_eq!(t1.title, "Track One");
        assert_eq!(t1.artist, "Artist A");
        assert!((t1.bpm.unwrap() - 128.0).abs() < f32::EPSILON);
        assert_eq!(t1.file_path, "C:/Musica/track1.mp3");
        assert_eq!(t1.cues.len(), 0);
    }

    #[test]
    fn test_parse_cues_from_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="6.6.0" />
  <COLLECTION Entries="1">
    <TRACK TrackID="1" Name="Track With Cues" Artist="DJ Artist" BPM="128.00"
      Location="file://localhost/C:/Musica/track.mp3">
      <POSITION_MARK Name="Memory Cue" Type="0" Start="15234" Num="-1" Colour="0xFF0000"/>
      <POSITION_MARK Name="Hot Cue 1" Type="0" Start="45678" Num="0" Colour="0x00FF00"/>
      <POSITION_MARK Name="Loop 1" Type="1" Start="78901" End="89012" Num="1" Colour="0x0000FF"/>
    </TRACK>
  </COLLECTION>
</DJ_PLAYLISTS>"#;

        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");
        std::fs::write(temp_file.path(), xml).expect("Falha ao escrever XML de teste");
        let tracks = RekordboxXmlParser::parse_tracks(temp_file.path()).expect("Falha ao parsear XML");

        assert_eq!(tracks.len(), 1);

        let t = &tracks[0];
        assert_eq!(t.cues.len(), 3);

        // Cue 1: Memory Cue
        let c1 = &t.cues[0];
        assert_eq!(c1.name, "Memory Cue");
        assert_eq!(c1.start_ms, 15234);
        assert_eq!(c1.end_ms, None);
        assert_eq!(c1.hot_cue_index, None);
        assert_eq!(c1.color_rgb, Some((255, 0, 0)));

        // Cue 2: Hot Cue 1
        let c2 = &t.cues[1];
        assert_eq!(c2.name, "Hot Cue 1");
        assert_eq!(c2.start_ms, 45678);
        assert_eq!(c2.hot_cue_index, Some(0));
        assert_eq!(c2.color_rgb, Some((0, 255, 0)));

        // Cue 3: Loop 1
        let c3 = &t.cues[2];
        assert_eq!(c3.name, "Loop 1");
        assert_eq!(c3.start_ms, 78901);
        assert_eq!(c3.end_ms, Some(89012));
        assert_eq!(c3.hot_cue_index, None);
        assert_eq!(c3.color_rgb, Some((0, 0, 255)));
    }

    #[test]
    fn test_read_library_missing_file() {
        let temp_file = NamedTempFile::new().expect("Falha ao criar arquivo temporário");
        let err = read_library(temp_file.path()).unwrap_err();
        assert!(matches!(err, XmlParseError::IoError(_)));
    }

    #[test]
    fn test_normalize_location_file_url() {
        assert_eq!(
            RekordboxXmlParser::normalize_location("file://localhost/C:/Musica/track.mp3"),
            "C:/Musica/track.mp3"
        );
        assert_eq!(
            RekordboxXmlParser::normalize_location("file:///D:/Musica/track.mp3"),
            "D:/Musica/track.mp3"
        );
    }
}
