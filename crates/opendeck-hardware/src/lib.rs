//! Hardware abstraction layer para controladores Pioneer DJ.
//!
//! ## Visão Geral
//!
//! Este crate fornece:
//! - Detecção USB de dispositivos Pioneer (VID 0x09E8)
//! - Mapeamento MIDI para DDJ-400 / DDJ-200 / DDJ-FLX4
//! - Abstração de deck (esquerdo/direito) com estado de controles
//! - Callback de eventos (botões, faders, jog wheels, knobs)
//!
//! ## Arquitetura
//!
//! ```text
//! HardwareManager
//!     ├── UsbScanner (rusb) — enumera dispositivos VID_09E8
//!     ├── MidiInput (midir) — recebe mensagens MIDI do controlador
//!     └── DeckState[x2] — estado espelhado de cada deck
//! ```
//!
//! ## Mapeamento DDJ-400 (Referência)
//!
//! | Controle | Canal MIDI | CC/Note | Função |
//! |----------|------------|---------|--------|
//! | Play/Pause | 1/2 | Note 0x0B | Play/Stop deck |
//! | Cue | 1/2 | Note 0x0C | Cue point / back-cue |
//! | Sync | 1/2 | Note 0x0D | Sync BPM |
//! | Shift | 1/2 | Note 0x0E | Shift modifier |
//! | Jog Wheel (touch) | 1/2 | Note 0x36 | Vinyl mode touch |
//! | Jog Wheel (rotate) | 1/2 | CC 0x38 | Scratch / pitch bend |
//! | Tempo Fader | 1/2 | CC 0x0E | Tempo adjust (±16%) |
//! | Volume Fader | 1/2 | CC 0x07 | Channel volume |
//! | Crossfader | 1 | CC 0x0F | Crossfader position |
//! | EQ High | 1/2 | CC 0x15 | EQ High kill/cut |
//! | EQ Mid | 1/2 | CC 0x16 | EQ Mid kill/cut |
//! | EQ Low | 1/2 | CC 0x17 | EQ Low kill/cut |
//! | Filter Knob | 1/2 | CC 0x1C | Filter frequency |
//! | Browse Encoder | 1 | CC 0x10 | Library browse |
//! | Load Button | 1/2 | Note 0x28 | Load track to deck |

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{MidiInput, MidiInputConnection};
use opendeck_core::Track;
use rusb::{Context, UsbContext};
use thiserror::Error;

/// Callback para eventos de hardware.
pub type HardwareCallback = Arc<dyn Fn(HardwareEvent) + Send + Sync>;

/// Vendor ID da Pioneer DJ.
pub const PIONEER_VID: u16 = 0x09E8;

/// Product IDs conhecidos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum PioneerPid {
    DDJ200 = 0x0084,
    DDJ400 = 0x0085,
    DDJFLX4 = 0x008A,
    DDJFLX6 = 0x008B,
    DDJREV1 = 0x008C,
    DDJREV7 = 0x008D,
    Unknown(u16),
}

impl PioneerPid {
    pub fn as_u16(&self) -> u16 {
        match self {
            PioneerPid::DDJ200 => 0x0084,
            PioneerPid::DDJ400 => 0x0085,
            PioneerPid::DDJFLX4 => 0x008A,
            PioneerPid::DDJFLX6 => 0x008B,
            PioneerPid::DDJREV1 => 0x008C,
            PioneerPid::DDJREV7 => 0x008D,
            PioneerPid::Unknown(v) => *v,
        }
    }
}

impl From<u16> for PioneerPid {
    fn from(pid: u16) -> Self {
        match pid {
            0x0084 => PioneerPid::DDJ200,
            0x0085 => PioneerPid::DDJ400,
            0x008A => PioneerPid::DDJFLX4,
            0x008B => PioneerPid::DDJFLX6,
            0x008C => PioneerPid::DDJREV1,
            0x008D => PioneerPid::DDJREV7,
            other => PioneerPid::Unknown(other),
        }
    }
}

/// Identifica um dispositivo Pioneer conectado via USB.
#[derive(Debug, Clone)]
pub struct PioneerDevice {
    pub vid: u16,
    pub pid: PioneerPid,
    pub bus_number: u8,
    pub device_address: u8,
    pub product_string: Option<String>,
    pub serial_number: Option<String>,
}

impl PioneerDevice {
    /// Tenta abrir o dispositivo para comunicação USB direta (se necessário).
    pub fn open(&self) -> Result<rusb::DeviceHandle<Context>, HardwareError> {
        let ctx = Context::new()?;
        let mut handle = None;
        
        for device in ctx.devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == self.vid && desc.product_id() == self.pid.as_u16() {
                let d_bus = device.bus_number();
                let d_addr = device.address();
                if d_bus == self.bus_number && d_addr == self.device_address {
                    handle = Some(device.open()?);
                    break;
                }
            }
        }
        
        handle.ok_or(HardwareError::DeviceNotFound)
    }
}

/// Scanner USB para dispositivos Pioneer.
pub struct UsbScanner {
    context: Context,
}

impl UsbScanner {
    /// Cria novo scanner USB.
    pub fn new() -> Result<Self, HardwareError> {
        Ok(Self {
            context: Context::new()?,
        })
    }

    /// Lista todos os dispositivos Pioneer conectados (VID 0x09E8).
    pub fn scan(&self) -> Result<Vec<PioneerDevice>, HardwareError> {
        let mut devices = Vec::new();
        
        for device in self.context.devices()?.iter() {
            let desc = device.device_descriptor()?;
            
            if desc.vendor_id() == PIONEER_VID {
                let pid = PioneerPid::from(desc.product_id());
                
                // Tenta ler strings do dispositivo
                let product_string = device
                    .open()
                    .ok()
                    .and_then(|h| {
                        let langs = h.read_languages(Duration::from_millis(1000)).ok()?;
                        let lang = langs.first().copied()?;
                        h.read_product_string(lang, &desc, Duration::from_millis(1000)).ok()
                    });
                
                let serial_number = device
                    .open()
                    .ok()
                    .and_then(|h| {
                        let langs = h.read_languages(Duration::from_millis(1000)).ok()?;
                        let lang = langs.first().copied()?;
                        h.read_serial_number_string(lang, &desc, Duration::from_millis(1000)).ok()
                    });
                
                devices.push(PioneerDevice {
                    vid: desc.vendor_id(),
                    pid,
                    bus_number: device.bus_number(),
                    device_address: device.address(),
                    product_string,
                    serial_number,
                });
            }
        }
        
        Ok(devices)
    }

    /// Retorna o primeiro DDJ-400 encontrado, se houver.
    pub fn find_ddj400(&self) -> Result<Option<PioneerDevice>, HardwareError> {
        let devices = self.scan()?;
        Ok(devices.into_iter().find(|d| d.pid == PioneerPid::DDJ400))
    }
}

/// Deck físico (esquerdo = 1, direito = 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeckId {
    #[default]
    Left = 1,
    Right = 2,
}

impl DeckId {
    /// Retorna o canal MIDI correspondente (1-based).
    pub fn midi_channel(&self) -> u8 {
        *self as u8
    }
}

/// Controles do deck mapeados para MIDI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeckControl {
    // Transport
    PlayPause,
    Cue,
    Sync,
    Shift,
    
    // Jog Wheel
    JogTouch,
    JogRotate,
    
    // Faders
    TempoFader,
    VolumeFader,
    Crossfader,
    
    // EQ / Filter
    EqHigh,
    EqMid,
    EqLow,
    Filter,
    
    // Browser
    BrowseEncoder,
    BrowsePress,
    
    // Hot Cues (1-8)
    HotCue1,
    HotCue2,
    HotCue3,
    HotCue4,
    HotCue5,
    HotCue6,
    HotCue7,
    HotCue8,
    
    // Load
    Load,
    
    // Loop
    LoopIn,
    LoopOut,
    LoopExit,
    
    // Sampler / Pad Mode
    PadMode1,
    PadMode2,
    PadMode3,
    PadMode4,
}

/// Mapeamento MIDI para DDJ-400.
/// 
/// Baseado na documentação oficial Pioneer e engenharia reversa da comunidade.
pub struct Ddj400Mapping;

impl Ddj400Mapping {
    /// Mapeia mensagem MIDI bruta para controle de deck.
    pub fn map_message(status: u8, data1: u8, data2: u8) -> Option<(DeckId, DeckControl, ControlValue)> {
        let channel = status & 0x0F; // 0-15
        let msg_type = status & 0xF0; // Note On/Off (0x90/0x80) ou CC (0xB0)
        let deck_id = match channel {
            0 => DeckId::Left,   // Canal 1 = deck esquerdo
            1 => DeckId::Right,  // Canal 2 = deck direito
            _ => return None,    // Canais 3+ não são decks
        };
        
        match msg_type {
            0x90 => { // Note On
                Self::map_note_on(deck_id, data1, data2)
            }
            0x80 => { // Note Off
                Self::map_note_off(deck_id, data1, data2)
            }
            0xB0 => { // Control Change
                Self::map_cc(deck_id, data1, data2)
            }
            _ => None,
        }
    }
    
    fn map_note_on(deck: DeckId, note: u8, velocity: u8) -> Option<(DeckId, DeckControl, ControlValue)> {
        let control = match note {
            0x0B => DeckControl::PlayPause,
            0x0C => DeckControl::Cue,
            0x0D => DeckControl::Sync,
            0x0E => DeckControl::Shift,
            0x36 => DeckControl::JogTouch,
            0x28 => DeckControl::Load,
            0x1D => DeckControl::HotCue1,
            0x1E => DeckControl::HotCue2,
            0x1F => DeckControl::HotCue3,
            0x20 => DeckControl::HotCue4,
            0x21 => DeckControl::HotCue5,
            0x22 => DeckControl::HotCue6,
            0x23 => DeckControl::HotCue7,
            0x24 => DeckControl::HotCue8,
            0x2D => DeckControl::LoopIn,
            0x2E => DeckControl::LoopOut,
            0x2F => DeckControl::LoopExit,
            0x30 => DeckControl::PadMode1,
            0x31 => DeckControl::PadMode2,
            0x32 => DeckControl::PadMode3,
            0x33 => DeckControl::PadMode4,
            _ => return None,
        };
        
        Some((deck, control, ControlValue::Button(velocity > 0)))
    }
    
    fn map_note_off(deck: DeckId, note: u8, _velocity: u8) -> Option<(DeckId, DeckControl, ControlValue)> {
        // Note Off para botões momentâneos
        let control = match note {
            0x0B => DeckControl::PlayPause,
            0x0C => DeckControl::Cue,
            0x0D => DeckControl::Sync,
            0x0E => DeckControl::Shift,
            0x36 => DeckControl::JogTouch,
            0x28 => DeckControl::Load,
            0x1D..=0x24 => return None, // Hot cues são toggle, não momentâneos
            0x2D => DeckControl::LoopIn,
            0x2E => DeckControl::LoopOut,
            0x2F => DeckControl::LoopExit,
            0x30..=0x33 => return None, // Pad modes são toggle
            _ => return None,
        };
        
        Some((deck, control, ControlValue::Button(false)))
    }
    
    fn map_cc(deck: DeckId, cc: u8, value: u8) -> Option<(DeckId, DeckControl, ControlValue)> {
        let control = match cc {
            0x38 => DeckControl::JogRotate,
            0x0E => DeckControl::TempoFader,
            0x07 => DeckControl::VolumeFader,
            0x0F => DeckControl::Crossfader, // Canal 1 apenas
            0x15 => DeckControl::EqHigh,
            0x16 => DeckControl::EqMid,
            0x17 => DeckControl::EqLow,
            0x1C => DeckControl::Filter,
            0x10 => DeckControl::BrowseEncoder,
            _ => return None,
        };
        
        // Crossfader só no canal 1 (deck esquerdo)
        if control == DeckControl::Crossfader && deck != DeckId::Left {
            return None;
        }
        
        Some((deck, control, ControlValue::Slider(value)))
    }
}

/// Valor de um controle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlValue {
    Button(bool),
    Slider(u8),      // 0-127
    Encoder(i8),     // -64 a +63 (relative)
    JogDelta(i16),   // Delta de rotação do jog
}

impl ControlValue {
    /// Normaliza slider 0-127 para 0.0-1.0.
    pub fn as_f32(&self) -> f32 {
        match self {
            ControlValue::Slider(v) => *v as f32 / 127.0,
            ControlValue::Button(b) => if *b { 1.0 } else { 0.0 },
            ControlValue::Encoder(v) => (*v as f32 + 64.0) / 127.0,
            ControlValue::JogDelta(v) => *v as f32 / 128.0,
        }
    }
    
    /// Converte tempo fader (0-127) para fator de tempo (±16% = 0.84 a 1.16).
    pub fn as_tempo_factor(&self) -> f32 {
        let normalized = self.as_f32();
        0.84 + (normalized * 0.32) // Centro (64) = 1.0
    }
    
    /// Converte EQ (0-127) para ganho dB (-inf a +6dB aprox).
    pub fn as_eq_gain_db(&self) -> f32 {
        let normalized = self.as_f32();
        // Curva logarítmica aproximada: centro (64) = 0dB
        if normalized < 0.5 {
            -48.0 + (normalized * 96.0) // -48dB a 0dB
        } else {
            (normalized - 0.5) * 12.0 // 0dB a +6dB
        }
    }
}

/// Estado de um deck.
#[derive(Debug, Clone)]
pub struct DeckState {
    pub deck_id: DeckId,
    pub track_loaded: Option<Track>,
    pub play_state: PlayState,
    pub tempo_factor: f32,
    pub volume: f32,
    pub eq_high: f32,
    pub eq_mid: f32,
    pub eq_low: f32,
    pub filter: f32,
    pub jog_touch: bool,
    pub shift_pressed: bool,
    pub hot_cues: [bool; 8],
    pub loop_active: bool,
}

impl Default for DeckState {
    fn default() -> Self {
        Self {
            deck_id: DeckId::default(),
            track_loaded: None,
            play_state: PlayState::default(),
            tempo_factor: 1.0,
            volume: 1.0,
            eq_high: 0.0,
            eq_mid: 0.0,
            eq_low: 0.0,
            filter: 0.5,
            jog_touch: false,
            shift_pressed: false,
            hot_cues: [false; 8],
            loop_active: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Evento de hardware unificado.
#[derive(Debug, Clone)]
pub enum HardwareEvent {
    DeviceConnected(PioneerDevice),
    DeviceDisconnected(PioneerDevice),
    DeckControl {
        deck: DeckId,
        control: DeckControl,
        value: ControlValue,
        timestamp: std::time::Instant,
    },
    MidiRaw {
        deck: DeckId,
        status: u8,
        data1: u8,
        data2: u8,
    },
}

/// Gerenciador principal de hardware.
/// 
/// Combina scanner USB + entrada MIDI + estado dos decks.
pub struct HardwareManager {
    usb_scanner: UsbScanner,
    midi_connection: Option<MidiInputConnection<()>>,
    deck_states: Arc<Mutex<HashMap<DeckId, DeckState>>>,
    callback: Option<HardwareCallback>,
    scan_thread: Option<thread::JoinHandle<()>>,
    running: Arc<Mutex<bool>>,
}

impl HardwareManager {
    /// Cria novo gerenciador de hardware.
    pub fn new() -> Result<Self, HardwareError> {
        Ok(Self {
            usb_scanner: UsbScanner::new()?,
            midi_connection: None,
            deck_states: Arc::new(Mutex::new({
                let mut m = HashMap::new();
                m.insert(DeckId::Left, DeckState { deck_id: DeckId::Left, ..Default::default() });
                m.insert(DeckId::Right, DeckState { deck_id: DeckId::Right, ..Default::default() });
                m
            })),
            callback: None,
            scan_thread: None,
            running: Arc::new(Mutex::new(false)),
        })
    }

    /// Define callback para eventos de hardware.
    pub fn set_callback<F>(&mut self, callback: F)
    where
        F: Fn(HardwareEvent) + Send + Sync + 'static,
    {
        self.callback = Some(Arc::new(callback));
    }

    /// Inicia monitoramento USB (polling a cada 2s) e MIDI.
    pub fn start(&mut self) -> Result<(), HardwareError> {
        *self.running.lock().unwrap() = true;
        
        // Inicia thread de scan USB
        let scanner = UsbScanner::new()?;
        let running = Arc::clone(&self.running);
        let callback = self.callback.clone();
        
        let scan_handle = thread::spawn(move || {
            let mut known_devices: HashMap<String, PioneerDevice> = HashMap::new();
            
            while *running.lock().unwrap() {
                if let Ok(devices) = scanner.scan() {
                    let mut current: HashMap<String, PioneerDevice> = HashMap::new();
                    
                    for device in devices {
                        let key = format!("{}:{}", device.bus_number, device.device_address);
                        current.insert(key.clone(), device.clone());
                        
                        if !known_devices.contains_key(&key) {
                            // Novo dispositivo conectado
                            if let Some(cb) = &callback {
                                cb(HardwareEvent::DeviceConnected(device.clone()));
                            }
                        }
                    }
                    
                    // Verifica desconexões
                    for (key, device) in known_devices.drain() {
                        if !current.contains_key(&key) {
                            if let Some(cb) = &callback {
                                cb(HardwareEvent::DeviceDisconnected(device));
                            }
                        }
                    }
                    
                    known_devices = current;
                }
                
                thread::sleep(Duration::from_secs(2));
            }
        });
        
        self.scan_thread = Some(scan_handle);
        
        // Inicia MIDI input
        self.start_midi()?;
        
        Ok(())
    }

    /// Inicia escuta MIDI.
    fn start_midi(&mut self) -> Result<(), HardwareError> {
        let midi_in = MidiInput::new("opendeck-hardware")?;
        
        // Lista portas disponíveis
        let ports = midi_in.ports();
        let port = ports.iter().find(|p| {
            let name = midi_in.port_name(p).unwrap_or_default();
            name.to_lowercase().contains("ddj") || name.to_lowercase().contains("pioneer")
        });
        
        let port = match port {
            Some(p) => p,
            None => return Err(HardwareError::MidiPortNotFound),
        };
        
        let deck_states = Arc::clone(&self.deck_states);
        let callback = self.callback.clone();
        
        let conn = midi_in.connect(port, "opendeck-midi", move |_stamp, msg, _| {
            if msg.len() >= 3 {
                let status = msg[0];
                let data1 = msg[1];
                let data2 = msg[2];
                
                if let Some((deck, control, value)) = Ddj400Mapping::map_message(status, data1, data2) {
                    // Atualiza estado do deck
                    if let Ok(mut states) = deck_states.lock() {
                        if let Some(state) = states.get_mut(&deck) {
                            Self::apply_control(state, control, value);
                        }
                    }
                    
                    // Emite evento
                    if let Some(cb) = &callback {
                        cb(HardwareEvent::DeckControl {
                            deck,
                            control,
                            value,
                            timestamp: std::time::Instant::now(),
                        });
                    }
                    
                    // Também emite raw
                    if let Some(cb) = &callback {
                        cb(HardwareEvent::MidiRaw { deck, status, data1, data2 });
                    }
                }
            }
        }, ())?;
        
        self.midi_connection = Some(conn);
        Ok(())
    }

    fn apply_control(state: &mut DeckState, control: DeckControl, value: ControlValue) {
        match control {
            DeckControl::PlayPause => {
                if let ControlValue::Button(true) = value {
                    state.play_state = match state.play_state {
                        PlayState::Stopped | PlayState::Paused => PlayState::Playing,
                        PlayState::Playing => PlayState::Paused,
                    };
                }
            }
            DeckControl::Cue => {
                if let ControlValue::Button(true) = value {
                    // Lógica de cue: se playing -> pausa e volta ao cue point
                    // se paused -> preview do cue point
                }
            }
            DeckControl::Sync => {
                if let ControlValue::Button(true) = value {
                    // Solicita sync de BPM (tratado pela camada superior)
                }
            }
            DeckControl::Shift => {
                if let ControlValue::Button(pressed) = value {
                    state.shift_pressed = pressed;
                }
            }
            DeckControl::JogTouch => {
                if let ControlValue::Button(touch) = value {
                    state.jog_touch = touch;
                }
            }
            DeckControl::JogRotate => {
                // Jog rotate tratado como pitch bend temporário
            }
            DeckControl::TempoFader => {
                state.tempo_factor = value.as_tempo_factor();
            }
            DeckControl::VolumeFader => {
                state.volume = value.as_f32();
            }
            DeckControl::Crossfader => {
                // Crossfader afeta volume relativo dos decks
            }
            DeckControl::EqHigh => {
                state.eq_high = value.as_eq_gain_db();
            }
            DeckControl::EqMid => {
                state.eq_mid = value.as_eq_gain_db();
            }
            DeckControl::EqLow => {
                state.eq_low = value.as_eq_gain_db();
            }
            DeckControl::Filter => {
                state.filter = value.as_f32(); // 0.0 a 1.0
            }
            DeckControl::HotCue1 => state.hot_cues[0] = value.as_f32() > 0.5,
            DeckControl::HotCue2 => state.hot_cues[1] = value.as_f32() > 0.5,
            DeckControl::HotCue3 => state.hot_cues[2] = value.as_f32() > 0.5,
            DeckControl::HotCue4 => state.hot_cues[3] = value.as_f32() > 0.5,
            DeckControl::HotCue5 => state.hot_cues[4] = value.as_f32() > 0.5,
            DeckControl::HotCue6 => state.hot_cues[5] = value.as_f32() > 0.5,
            DeckControl::HotCue7 => state.hot_cues[6] = value.as_f32() > 0.5,
            DeckControl::HotCue8 => state.hot_cues[7] = value.as_f32() > 0.5,
            DeckControl::Load => {
                // Load track solicitado - camada superior decide qual track
            }
            _ => {}
        }
    }

    /// Retorna estado atual de um deck.
    pub fn get_deck_state(&self, deck: DeckId) -> Option<DeckState> {
        self.deck_states.lock().ok()?.get(&deck).cloned()
    }

    /// Retorna estados de ambos os decks.
    pub fn get_all_deck_states(&self) -> HashMap<DeckId, DeckState> {
        self.deck_states.lock().unwrap().clone()
    }

    /// Carrega track em um deck (atualiza estado local).
    pub fn load_track(&self, deck: DeckId, track: Track) {
        if let Ok(mut states) = self.deck_states.lock() {
            if let Some(state) = states.get_mut(&deck) {
                state.track_loaded = Some(track);
                state.play_state = PlayState::Stopped;
            }
        }
    }

    /// Para monitoramento.
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        if let Some(handle) = self.scan_thread.take() {
            let _ = handle.join();
        }
        self.midi_connection = None;
    }

    /// Lista dispositivos Pioneer conectados atualmente.
    pub fn list_devices(&self) -> Result<Vec<PioneerDevice>, HardwareError> {
        self.usb_scanner.scan()
    }
}

impl Drop for HardwareManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Erros de hardware.
#[derive(Error, Debug)]
pub enum HardwareError {
    #[error("Erro USB: {0}")]
    Usb(#[from] rusb::Error),
    
    #[error("Erro MIDI: {0}")]
    Midi(#[from] midir::ConnectError<midir::MidiInput>),
    
    #[error("Erro de inicialização MIDI: {0}")]
    MidiInit(#[from] midir::InitError),
    
    #[error("Dispositivo não encontrado")]
    DeviceNotFound,
    
    #[error("Porta MIDI Pioneer não encontrada")]
    MidiPortNotFound,
    
    #[error("Erro de I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pioneer_pid_from_u16() {
        assert_eq!(PioneerPid::from(0x0084), PioneerPid::DDJ200);
        assert_eq!(PioneerPid::from(0x0085), PioneerPid::DDJ400);
        assert_eq!(PioneerPid::from(0x008A), PioneerPid::DDJFLX4);
        assert_eq!(PioneerPid::from(0x9999), PioneerPid::Unknown(0x9999));
    }

    #[test]
    fn test_deck_id_midi_channel() {
        assert_eq!(DeckId::Left.midi_channel(), 1);
        assert_eq!(DeckId::Right.midi_channel(), 2);
    }

    #[test]
    fn test_ddj400_mapping_play_pause() {
        // Note On channel 1 (deck left), note 0x0B, velocity 127
        let result = Ddj400Mapping::map_message(0x90, 0x0B, 0x7F);
        assert!(result.is_some());
        let (deck, control, value) = result.unwrap();
        assert_eq!(deck, DeckId::Left);
        assert_eq!(control, DeckControl::PlayPause);
        assert_eq!(value, ControlValue::Button(true));
    }

    #[test]
    fn test_ddj400_mapping_play_pause_off() {
        // Note Off channel 1, note 0x0B
        let result = Ddj400Mapping::map_message(0x80, 0x0B, 0x00);
        assert!(result.is_some());
        let (deck, control, value) = result.unwrap();
        assert_eq!(deck, DeckId::Left);
        assert_eq!(control, DeckControl::PlayPause);
        assert_eq!(value, ControlValue::Button(false));
    }

    #[test]
    fn test_ddj400_mapping_cue() {
        let result = Ddj400Mapping::map_message(0x91, 0x0C, 0x7F); // Canal 2 (right)
        assert!(result.is_some());
        let (deck, control, value) = result.unwrap();
        assert_eq!(deck, DeckId::Right);
        assert_eq!(control, DeckControl::Cue);
        assert_eq!(value, ControlValue::Button(true));
    }

    #[test]
    fn test_ddj400_mapping_tempo_fader() {
        // CC channel 1, CC 0x0E, value 64 (centro)
        let result = Ddj400Mapping::map_message(0xB0, 0x0E, 0x40);
        assert!(result.is_some());
        let (deck, control, value) = result.unwrap();
        assert_eq!(deck, DeckId::Left);
        assert_eq!(control, DeckControl::TempoFader);
        assert_eq!(value, ControlValue::Slider(64));
        
        // Centro deve dar fator 1.0
        assert!((value.as_tempo_factor() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ddj400_mapping_tempo_fader_extremes() {
        // Valor mínimo = 0.84x
        let result = Ddj400Mapping::map_message(0xB0, 0x0E, 0x00);
        let (_, _, value) = result.unwrap();
        assert!((value.as_tempo_factor() - 0.84).abs() < 0.01);
        
        // Valor máximo = 1.16x
        let result = Ddj400Mapping::map_message(0xB0, 0x0E, 0x7F);
        let (_, _, value) = result.unwrap();
        assert!((value.as_tempo_factor() - 1.16).abs() < 0.01);
    }

    #[test]
    fn test_ddj400_mapping_volume_fader() {
        let result = Ddj400Mapping::map_message(0xB0, 0x07, 0x7F);
        assert!(result.is_some());
        let (_, control, value) = result.unwrap();
        assert_eq!(control, DeckControl::VolumeFader);
        assert!((value.as_f32() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_ddj400_mapping_eq() {
        // EQ High
        let result = Ddj400Mapping::map_message(0xB0, 0x15, 0x40); // Centro
        let (_, control, value) = result.unwrap();
        assert_eq!(control, DeckControl::EqHigh);
        // Centro = 0dB
        assert!((value.as_eq_gain_db() - 0.0).abs() < 1.0);
        
        // EQ Mid
        let result = Ddj400Mapping::map_message(0xB0, 0x16, 0x00); // Min
        let (_, control, value) = result.unwrap();
        assert_eq!(control, DeckControl::EqMid);
        assert!((value.as_eq_gain_db() - (-48.0)).abs() < 1.0);
    }

    #[test]
    fn test_ddj400_mapping_crossfader_only_left() {
        // Crossfader no canal esquerdo
        let result = Ddj400Mapping::map_message(0xB0, 0x0F, 0x40);
        assert!(result.is_some());
        
        // Crossfader no canal direito deve ser ignorado
        let result = Ddj400Mapping::map_message(0xB1, 0x0F, 0x40);
        assert!(result.is_none());
    }

    #[test]
    fn test_ddj400_mapping_jog_rotate() {
        let result = Ddj400Mapping::map_message(0xB0, 0x38, 0x40);
        assert!(result.is_some());
        let (_, control, _) = result.unwrap();
        assert_eq!(control, DeckControl::JogRotate);
    }

    #[test]
    fn test_ddj400_mapping_hot_cues() {
        for i in 0..8 {
            let note = 0x1D + i;
            let result = Ddj400Mapping::map_message(0x90, note, 0x7F);
            assert!(result.is_some(), "Hot cue {} falhou", i+1);
            let (_, control, _) = result.unwrap();
            assert!(matches!(control, DeckControl::HotCue1 | DeckControl::HotCue2 | 
                                   DeckControl::HotCue3 | DeckControl::HotCue4 |
                                   DeckControl::HotCue5 | DeckControl::HotCue6 |
                                   DeckControl::HotCue7 | DeckControl::HotCue8));
        }
    }

    #[test]
    fn test_ddj400_mapping_unknown_channel() {
        // Canal 3 (index 2) não é deck
        let result = Ddj400Mapping::map_message(0x92, 0x0B, 0x7F);
        assert!(result.is_none());
    }

    #[test]
    fn test_control_value_as_f32() {
        assert_eq!(ControlValue::Slider(0).as_f32(), 0.0);
        assert_eq!(ControlValue::Slider(127).as_f32(), 1.0);
        assert!((ControlValue::Slider(64).as_f32() - 0.5).abs() < 0.01);
        assert_eq!(ControlValue::Button(true).as_f32(), 1.0);
        assert_eq!(ControlValue::Button(false).as_f32(), 0.0);
    }

    #[test]
    fn test_usb_scanner_creation() {
        let scanner = UsbScanner::new();
        assert!(scanner.is_ok());
    }

    #[test]
    fn test_hardware_manager_creation() {
        let manager = HardwareManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_deck_state_default() {
        let state = DeckState::default();
        assert_eq!(state.play_state, PlayState::Stopped);
        assert_eq!(state.tempo_factor, 1.0);
        assert_eq!(state.volume, 1.0);
        assert!(!state.shift_pressed);
        assert!(!state.jog_touch);
        assert!(!state.loop_active);
        assert_eq!(state.hot_cues, [false; 8]);
    }
}