interface Track {
  id: string;
  title: string;
  artist: string;
  bpm: number | null;
  file_path: string;
  analysis_path: string | null;
  cues: CuePoint[];
  analysis: TrackAnalysis | null;
}

interface CuePoint {
  id: string;
  track_id: string;
  name: string;
  cue_type: 'Memory' | 'HotCue' | 'Loop' | 'FadeIn' | 'FadeOut' | 'Unknown';
  start_ms: number;
  end_ms: number | null;
  hot_cue_index: number | null;
  color_rgb: [number, number, number] | null;
}

interface TrackAnalysis {
  beatgrid_seconds: number[];
  waveform_amplitudes: number[];
  estimated_bpm: number | null;
}

interface DeviceInfo {
  vid: number;
  pid: number;
  bus_number: number;
  device_address: number;
  product_string: string | null;
  serial_number: string | null;
}

interface Playlist {
  id: string;
  name: string;
  track_count: number;
}

interface RecentTrack {
  track_id: string;
  title: string;
  artist: string;
  file_path: string;
  timestamp: number;
}

interface UserPreferences {
  master_volume: number;
  theme: string;
  auto_analyze: boolean;
  audio_buffer_seconds: number;
  show_waveform: boolean;
  default_deck: number;
}

interface UserState {
  last_usb_root: string | null;
  recent_tracks: RecentTrack[];
  window_width: number | null;
  window_height: number | null;
  window_x: number | null;
  window_y: number | null;
  preferences: UserPreferences;
}

interface PlaybackState {
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  tempoFactor: number;
  pitchSemitones: number;
}

interface AuthState {
  isAuthenticated: boolean;
  token: string | null;
  login: (token: string) => void;
  logout: () => void;
}

export type { Track, CuePoint, TrackAnalysis, DeviceInfo, Playlist, PlaybackState, AuthState, UserState, UserPreferences, RecentTrack };
