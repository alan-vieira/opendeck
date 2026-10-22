use opendeck_db::{read_library, XmlParseError};
use opendeck_db::parser::RekordboxXmlParser;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_parse_sample_xml_tracks() {
    let path = fixture_path("rekordbox_sample.xml");
    let tracks = RekordboxXmlParser::parse_tracks(&path).expect("Falha ao parsear XML de exemplo");

    assert_eq!(tracks.len(), 3, "Esperava 3 tracks, obteve {}", tracks.len());

    // Track 1: Midnight City - M83
    let t1 = &tracks[0];
    assert_eq!(t1.id, "1");
    assert_eq!(t1.title, "Midnight City");
    assert_eq!(t1.artist, "M83");
    assert!((t1.bpm.unwrap() - 105.5).abs() < f32::EPSILON);
    assert_eq!(t1.file_path, "C:/Music/M83/Midnight%20City.mp3");

    // Track 2: Strobe - Deadmau5 (caminho Unix file:///)
    let t2 = &tracks[1];
    assert_eq!(t2.id, "2");
    assert_eq!(t2.title, "Strobe");
    assert_eq!(t2.artist, "Deadmau5");
    assert!((t2.bpm.unwrap() - 128.0).abs() < f32::EPSILON);
    assert_eq!(t2.file_path, "Users/dj/Music/Deadmau5/Strobe.wav");

    // Track 3: Unknown Track (sem BPM no XML)
    let t3 = &tracks[2];
    assert_eq!(t3.id, "3");
    assert_eq!(t3.title, "Unknown Track");
    assert_eq!(t3.artist, "");
    assert!(t3.bpm.is_none());
    assert_eq!(t3.file_path, "D:/Downloads/unknown.mp3");
}

#[test]
fn test_read_library_from_usb_root() {
    let temp_dir = tempfile::TempDir::new().expect("Falha ao criar diretório temporário");
    let pioneer_dir = temp_dir.path().join("PIONEER").join("rekordbox");
    std::fs::create_dir_all(&pioneer_dir).expect("Falha ao criar diretório PIONEER");

    let fixture = fixture_path("rekordbox_sample.xml");
    let xml_content = std::fs::read_to_string(&fixture).expect("Falha ao ler fixture");
    let xml_path = pioneer_dir.join("rekordbox.xml");
    std::fs::write(&xml_path, xml_content).expect("Falha ao escrever XML");

    let tracks = read_library(temp_dir.path()).expect("Falha ao ler biblioteca");
    assert_eq!(tracks.len(), 3);
}

#[test]
fn test_read_library_missing_xml() {
    let temp_dir = tempfile::TempDir::new().expect("Falha ao criar diretório temporário");
    let err = read_library(temp_dir.path()).unwrap_err();
    assert!(matches!(err, XmlParseError::IoError(_)));
}

#[test]
fn test_read_library_missing_pioneer_dir() {
    let temp_dir = tempfile::TempDir::new().expect("Falha ao criar diretório temporário");
    std::fs::create_dir_all(temp_dir.path().join("PIONEER")).expect("Falha ao criar PIONEER");
    let err = read_library(temp_dir.path()).unwrap_err();
    assert!(matches!(err, XmlParseError::IoError(_)));
}

#[test]
fn test_playlist_not_implemented() {
    let temp_dir = tempfile::TempDir::new().expect("Falha ao criar diretório temporário");
    let err = opendeck_db::read_playlist(temp_dir.path(), "My Playlist").unwrap_err();
    assert!(matches!(err, XmlParseError::UnsupportedFormat));
}

#[test]
fn test_normalize_location_edge_cases() {
    assert_eq!(
        RekordboxXmlParser::normalize_location("file://localhost/C:/Music/track.mp3"),
        "C:/Music/track.mp3"
    );
    assert_eq!(
        RekordboxXmlParser::normalize_location("file:///Users/dj/Music/track.wav"),
        "Users/dj/Music/track.wav"
    );
    assert_eq!(
        RekordboxXmlParser::normalize_location("file://localhost/D:/Downloads/track.mp3"),
        "D:/Downloads/track.mp3"
    );
    assert_eq!(
        RekordboxXmlParser::normalize_location("C:/Music/track.mp3"),
        "C:/Music/track.mp3"
    );
}
