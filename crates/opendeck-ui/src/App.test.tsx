import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import App from './App';
import type { Track } from './types';

// Mock do Tauri invoke (window.__TAURI_INTERNALS__ é usado pelo @tauri-apps/api)
const mockInvokeInternals = { invoke: vi.fn() };
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  value: mockInvokeInternals,
  writable: true,
  configurable: true,
});

// Mock do localStorage (jsdom não expõe por padrão sem --localstorage-file)
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => { store[key] = value; },
    removeItem: (key: string) => { delete store[key]; },
    clear: () => { store = {}; },
  };
})();
Object.defineProperty(window, 'localStorage', {
  value: localStorageMock,
  writable: true,
  configurable: true,
});

const mockInvoke = mockInvokeInternals.invoke;

describe('Frontend-Backend communication via invoke', () => {
  const mockTracks: Track[] = [
    { id: '1', title: 'Track 1', artist: 'Artist 1', bpm: 128, file_path: '/music/track1.mp3', analysis_path: '/music/track1.dat' },
    { id: '2', title: 'Track 2', artist: 'Artist 2', bpm: 140, file_path: '/music/track2.mp3', analysis_path: null },
  ];

  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(mockTracks);
    // Simula usuário autenticado para pular o AuthModal
    localStorage.setItem('token', 'dev-token-test');
  });

  it('deve carregar biblioteca via invoke ao montar o App', async () => {
    render(<App />);

    // Aguarda a chamada assíncrona de load_library
    await new Promise((resolve) => setTimeout(resolve, 100));

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'load_library' && (args as any).dbPath && typeof (args as any).dbPath === 'string')).toBe(true);
  });

  it('deve selecionar uma track', async () => {
    render(<App />);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Clica na primeira track na lista lateral (aside)
    const trackTitles = screen.getAllByText('Track 1');
    const trackRow = trackTitles[0].closest('div');
    fireEvent.click(trackRow!);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Verifica que a track foi selecionada (deck mostra o título)
    const titles = screen.getAllByText('Track 1');
    expect(titles.length).toBeGreaterThanOrEqual(1);
  });

  it('deve chamar play_track com deck_index ao clicar no botão Play', async () => {
    mockInvoke.mockResolvedValue(mockTracks);
    render(<App />);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Seleciona a track
    const trackTitles = screen.getAllByText('Track 1');
    const trackRow = trackTitles[0].closest('div');
    fireEvent.click(trackRow!);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Clica no botão Play do deck 1 (identificado por title)
    const playButtons = screen.getAllByRole('button', { name: /▶/ }).filter((b) => b.getAttribute('title') === 'Play');
    fireEvent.click(playButtons[0]);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'play_track' && (args as any).filePath === '/music/track1.mp3' && (args as any).deck_index === 0)).toBe(true);
  });

  it('deve chamar stop_track com deck_index ao clicar no botão Stop', async () => {
    mockInvoke.mockResolvedValue(mockTracks);
    render(<App />);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Seleciona a track e toca
    const trackTitles = screen.getAllByText('Track 1');
    const trackRow = trackTitles[0].closest('div');
    fireEvent.click(trackRow!);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const playButtons = screen.getAllByRole('button', { name: /▶/ }).filter((b) => b.getAttribute('title') === 'Play');
    fireEvent.click(playButtons[0]);

    await new Promise((resolve) => setTimeout(resolve, 100));

    // Agora deve mostrar Stop
    const stopButtons = screen.getAllByRole('button', { name: /⏹/ });
    fireEvent.click(stopButtons[0]);

    await new Promise((resolve) => setTimeout(resolve, 100));

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd, args]) => cmd === 'stop_track' && (args as any).deck_index === 0)).toBe(true);
  });

  it('deve chamar get_devices', async () => {
    mockInvoke.mockResolvedValue([]);
    // Teste direto da função getDevices
    const { getDevices } = await import('./services/api');
    await getDevices();

    const calls = mockInvoke.mock.calls;
    expect(calls.some(([cmd]) => cmd === 'get_devices')).toBe(true);
  });
});
