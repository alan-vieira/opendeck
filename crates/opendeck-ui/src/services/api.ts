import { invoke } from '@tauri-apps/api/core';
import type { Track, TrackAnalysis, CuePoint, DeviceInfo } from '../types';

export const loadLibrary = async (dbPath: string): Promise<Track[]> => {
  return invoke('load_library', { dbPath });
};

export const loadTrackAnalysis = async (datPath: string): Promise<TrackAnalysis> => {
  return invoke('load_track_analysis', { datPath });
};

export const getCuesForTrack = async (dbPath: string, trackId: string): Promise<CuePoint[]> => {
  return invoke('get_cues_for_track', { dbPath, track_id: trackId });
};

export const playTrack = async (filePath: string, deckIndex: number): Promise<void> => {
  return invoke('play_track', { filePath, deck_index: deckIndex });
};

export const stopTrack = async (deckIndex: number): Promise<void> => {
  return invoke('stop_track', { deck_index: deckIndex });
};

export const setTempo = async (factor: number, deckIndex: number): Promise<void> => {
  return invoke('set_tempo', { factor, deck_index: deckIndex });
};

export const setPitch = async (semitones: number, deckIndex: number): Promise<void> => {
  return invoke('set_pitch', { semitones, deck_index: deckIndex });
};

export const setVolume = async (volume: number, deckIndex: number): Promise<void> => {
  return invoke('set_volume', { volume, deck_index: deckIndex });
};

export const getDevices = async (): Promise<DeviceInfo[]> => {
  return invoke('get_devices', {});
};

export const login = async (_username: string, _password: string): Promise<string> => {
  // Placeholder: o backend Tauri pode expor um comando de login no futuro
  const token = `dev-token-${_username}`;
  localStorage.setItem('token', token);
  return token;
};

export const logout = (): void => {
  localStorage.removeItem('token');
};

export default { loadLibrary, loadTrackAnalysis, playTrack, stopTrack };
