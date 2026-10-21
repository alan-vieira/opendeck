import { describe, it, expect, vi, beforeEach } from 'vitest';
import { loadLibrary, loadTrackAnalysis, playTrack, stopTrack, setTempo, setPitch, setVolume, getDevices } from './api';

// Mock do Tauri invoke (window.__TAURI_INTERNALS__ é usado pelo @tauri-apps/api)
const mockInvokeInternals = { invoke: vi.fn() };
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  value: mockInvokeInternals,
  writable: true,
  configurable: true,
});

const mockInvoke = mockInvokeInternals.invoke;

describe('load_library command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com os parâmetros corretos', async () => {
    const mockTracks = [
      { id: '1', title: 'Track 1', artist: 'Artist 1', bpm: 128, file_path: '/path/track1.mp3', analysis_path: null },
    ];
    mockInvoke.mockResolvedValue(mockTracks);

    const result = await loadLibrary('C:/music/master.db');

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'load_library' && (args as any).dbPath === 'C:/music/master.db')).toBe(true);
    expect(result).toEqual(mockTracks);
  });

  it('deve propagar erros do invoke', async () => {
    mockInvoke.mockRejectedValue(new Error('Database error'));

    await expect(loadLibrary('C:/bad.db')).rejects.toThrow('Database error');
  });
});

describe('load_track_analysis command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com o caminho do DAT', async () => {
    const mockAnalysis = {
      beatgrid_seconds: [0, 4.56, 9.12],
      waveform_amplitudes: [0, 128, 255, 128, 0],
    };
    mockInvoke.mockResolvedValue(mockAnalysis);

    const result = await loadTrackAnalysis('C:/music/track.dat');

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'load_track_analysis' && (args as any).datPath === 'C:/music/track.dat')).toBe(true);
    expect(result.beatgrid_seconds).toEqual([0, 4.56, 9.12]);
    expect(result.waveform_amplitudes).toEqual([0, 128, 255, 128, 0]);
  });
});

describe('play_track command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com o caminho do arquivo e deck_index', async () => {
    mockInvoke.mockResolvedValue(undefined);

    await playTrack('C:/music/track.mp3', 0);

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'play_track' && (args as any).filePath === 'C:/music/track.mp3' && (args as any).deck_index === 0)).toBe(true);
  });
});

describe('stop_track command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com deck_index', async () => {
    mockInvoke.mockResolvedValue(undefined);

    await stopTrack(1);

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'stop_track' && (args as any).deck_index === 1)).toBe(true);
  });
});

describe('set_tempo command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com fator e deck_index', async () => {
    mockInvoke.mockResolvedValue(undefined);

    await setTempo(1.5, 0);

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'set_tempo' && (args as any).factor === 1.5 && (args as any).deck_index === 0)).toBe(true);
  });
});

describe('set_pitch command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com semitons e deck_index', async () => {
    mockInvoke.mockResolvedValue(undefined);

    await setPitch(2, 1);

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'set_pitch' && (args as any).semitones === 2 && (args as any).deck_index === 1)).toBe(true);
  });
});

describe('set_volume command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve chamar invoke com volume e deck_index', async () => {
    mockInvoke.mockResolvedValue(undefined);

    await setVolume(0.8, 0);

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'set_volume' && (args as any).volume === 0.8 && (args as any).deck_index === 0)).toBe(true);
  });
});

describe('get_devices command', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('deve retornar lista de dispositivos Pioneer', async () => {
    const mockDevices = [
      { vid: 0x09E8, pid: 0x0085, bus_number: 1, device_address: 2, product_string: 'DDJ-400', serial_number: '12345' },
    ];
    mockInvoke.mockResolvedValue(mockDevices);

    const result = await getDevices();

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd]) => cmd === 'get_devices')).toBe(true);
    expect(result).toHaveLength(1);
    expect(result[0].vid).toBe(0x09E8);
    expect(result[0].product_string).toBe('DDJ-400');
  });
});
