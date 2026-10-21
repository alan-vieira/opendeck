interface Track {
  id: string;
  title: string;
  artist: string;
  bpm: number | null;
  file_path: string;
  analysis_path: string | null;
}

interface CuePoint {
  id: string;
  track_id: string;
  time_seconds: number;
  cue_type: 'Normal' | 'LoopIn' | 'LoopOut' | 'Memory';
  name: string | null;
}

interface TrackAnalysis {
  beatgrid_seconds: number[];
  waveform_amplitudes: number[];
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

export type { Track, CuePoint, TrackAnalysis, DeviceInfo, Playlist, PlaybackState, AuthState };
