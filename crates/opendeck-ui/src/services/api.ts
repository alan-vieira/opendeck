import { invoke } from '@tauri-apps/api/core';
import type { Track, TrackAnalysis, CuePoint, DeviceInfo, UserState, UserPreferences } from '../types';

export const loadLibrary = async (usbRoot: string): Promise<Track[]> => {
  return invoke('load_library', { usbRoot }); 
};

export const loadTrackAnalysis = async (filePath: string): Promise<TrackAnalysis> => {
  return invoke('load_track_analysis', { filePath });
};

export const getCuesForTrack = async (trackId: string): Promise<CuePoint[]> => {
  return invoke('get_cues_for_track', { trackId });
};

export const playTrack = async (filePath: string, deckIndex: number): Promise<void> => {
  return invoke('play_track', { filePath, deckIndex }); 
};

export const pauseTrack = async (deckIndex: number): Promise<void> => {
  return invoke('pause_track', { deckIndex });
};

export const resumeTrack = async (deckIndex: number): Promise<void> => {
  return invoke('resume_track', { deckIndex });
};

export const seekTrack = async (deckIndex: number, positionMs: number): Promise<void> => {
  return invoke('seek_track', { deckIndex, positionMs });
};

export const stopTrack = async (deckIndex: number): Promise<void> => {
  return invoke('stop_track', { deckIndex });
};

export const setTempo = async (factor: number, deckIndex: number): Promise<void> => {
  return invoke('set_tempo', { factor, deckIndex });
};

export const setPitch = async (semitones: number, deckIndex: number): Promise<void> => {
  return invoke('set_pitch', { semitones, deckIndex });
};

export const setVolume = async (volume: number, deckIndex: number): Promise<void> => {
  return invoke('set_volume', { volume, deckIndex });
};

export const getDevices = async (): Promise<DeviceInfo[]> => {
  return invoke('get_devices', {});
};

export const getUserState = async (): Promise<UserState> => {
  return invoke('get_user_state', {});
};

export const saveUserState = async (userState: UserState): Promise<void> => {
  return invoke('save_user_state_cmd', { userState });
};

export const updatePreferences = async (preferences: UserPreferences): Promise<void> => {
  return invoke('update_preferences', { preferences });
};

export const updateWindowState = async (width: number, height: number, x: number, y: number): Promise<void> => {
  return invoke('update_window_state_cmd', { width, height, x, y });
};

export const login = async (_username: string, _password: string): Promise<string> => {
  const token = `dev-token-${_username}`;
  localStorage.setItem('token', token);
  return token;
};

export const logout = (): void => {
  localStorage.removeItem('token');
};

export default { loadLibrary, loadTrackAnalysis, playTrack, stopTrack };