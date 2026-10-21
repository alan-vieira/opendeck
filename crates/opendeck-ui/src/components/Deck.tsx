import { useState, useCallback, useEffect } from 'react';
import type { Track } from '../types';
import { playTrack, stopTrack, setTempo, setPitch, setVolume, getCuesForTrack } from '../services/api';

const DECK_LABELS = { 1: 'DECK 1', 2: 'DECK 2' };
const DECK_ACCENTS = { 1: '#3b82f6', 2: '#f59e0b' };

interface DeckProps {
  deckId: 1 | 2;
  track: Track | null;
  onLoadTrack: (track: Track) => void;
}

export default function Deck({ deckId, track, onLoadTrack }: DeckProps) {
  const [isPlaying, setIsPlaying] = useState(false);
  const [tempoFactor, setTempoFactor] = useState(1.0);
  const [pitchSemitones, setPitchSemitones] = useState(0);
  const [volumeLevel, setVolumeLevel] = useState(1.0);
  const [cues, setCues] = useState<Array<{ time: number; label: string }>>([]);
  const accent = DECK_ACCENTS[deckId];
  const label = DECK_LABELS[deckId];

  const handleLoad = useCallback(() => {
    if (track) {
      onLoadTrack(track);
      playTrack(track.file_path, deckId - 1).then(() => setIsPlaying(true));
    }
  }, [track, deckId, onLoadTrack]);

  const handlePlayPause = useCallback(async () => {
    if (!track) return;
    if (isPlaying) {
      await stopTrack(deckId - 1);
      setIsPlaying(false);
    } else {
      await playTrack(track.file_path, deckId - 1);
      setIsPlaying(true);
    }
  }, [track, deckId, isPlaying]);

  const handleStop = useCallback(async () => {
    await stopTrack(deckId - 1);
    setIsPlaying(false);
  }, [deckId]);

  const handleSetTempo = useCallback(async (factor: number) => {
    setTempoFactor(factor);
    await setTempo(factor, deckId - 1);
  }, [deckId]);

  const handleSetPitch = useCallback(async (semitones: number) => {
    setPitchSemitones(semitones);
    await setPitch(semitones, deckId - 1);
  }, [deckId]);

  const handleSetVolume = useCallback(async (vol: number) => {
    setVolumeLevel(vol);
    await setVolume(vol, deckId - 1);
  }, [deckId]);

  useEffect(() => {
    if (!track) return;
    getCuesForTrack('C:/Users/alsvi/AppData/Roaming/Pioneer/rekordbox/master.db', track.id)
      .then((loadedCues) => {
        setCues(
          loadedCues.map((c) => ({
            time: c.time_seconds,
            label: c.name || `Cue ${c.id.slice(-4)}`,
          }))
        );
      })
      .catch(() => setCues([]));
  }, [track?.id]);

  return (
    <div className="flex-1 flex flex-col bg-[#111111] border border-[#1f1f1f] rounded-lg overflow-hidden relative group">
      {/* Cabeçalho do deck */}
      <div className={`px-3 py-2 border-b border-[#1f1f1f] flex items-center justify-between bg-[#1a1a1a]`}>
        <div className="flex items-center gap-2">
          <span className={`text-xs font-bold tracking-wider text-[#8a8a8a]`}>{label}</span>
          <span className={`w-2 h-2 rounded-full bg-${accent === '#3b82f6' ? 'blue-500' : 'amber-500'}`} />
        </div>
        <div className="text-right">
          <div className="text-xs font-mono text-[#fafafa]">{tempoFactor.toFixed(2)}x</div>
          <div className="text-[10px] text-[#666]">{pitchSemitones >= 0 ? '+' : ''}${pitchSemitones}</div>
        </div>
      </div>

      {/* Info da track */}
      <div className="px-3 py-2 border-b border-[#1f1f1f] flex items-center gap-2 cursor-pointer hover:bg-[#1a1a1a] transition-colors onClick={handleLoad}">
        <div className="w-10 h-10 rounded bg-[#333] flex items-center justify-center text-xs font-bold text-[#666] shrink-0">
          {track?.title.charAt(0).toUpperCase() || '—'}
        </div>
        <div className="flex-1 min-w-0 overflow-hidden">
          <div className="text-sm font-medium truncate text-[#fafafa]">{track?.title || 'Sem track carregada'}</div>
          <div className="text-xs text-[#8a8a8a] truncate">{track?.artist || ''}</div>
          {track?.bpm && <div className="text-xs text-[#8a8a8a] font-mono">{track.bpm.toFixed(0)} BPM</div>}
        </div>
        <button
          onClick={(e) => { e.stopPropagation(); handleLoad(); }}
          className="p-1.5 text-[#666] hover:text-white transition-colors"
          title="Carregar"
        >
          ▶
        </button>
      </div>

      {/* Controles de transporte */}
      <div className="px-3 py-2 border-t border-[#1f1f1f] flex items-center justify-between bg-[#1a1a1a]">
        <div className="flex items-center gap-2">
          <button
            onClick={handlePlayPause}
            className={`w-10 h-10 rounded-full flex items-center justify-center transition-all active:scale-95 ${
              isPlaying
                ? 'bg-[#ef4444] text-white'
                : 'bg-[#3b82f6] text-white hover:bg-[#2563eb]'
            }`}
            title={isPlaying ? 'Pausar' : 'Play'}
          >
            {isPlaying ? '❚❚' : '▶'}
          </button>
          <button
            onClick={handleStop}
            className="w-10 h-10 rounded-full flex items-center justify-center bg-[#666] text-white hover:bg-[#ef4444] transition-colors active:scale-95"
            title="Stop"
          >
            ⏹
          </button>
          <button
            onClick={() => {}}
            className="w-10 h-10 rounded-full flex items-center justify-center bg-[#333] text-[#ccc] hover:text-white transition-colors"
            title="Cue"
          >
            Cue
          </button>
        </div>

        <div className="flex items-center gap-3">
          <div className="flex flex-col items-center gap-1">
            <label className="text-[10px] text-[#666]">Tempo</label>
            <input
              type="range"
              min="25"
              max="400"
              step="1"
              value={tempoFactor * 100}
              onChange={(e) => handleSetTempo(Number(e.target.value) / 100)}
              className="w-24 h-1 bg-[#444] rounded-lg appearance-none cursor-pointer accent-[#f59e0b]"
            />
          </div>
          <div className="flex flex-col items-center gap-1">
            <label className="text-[10px] text-[#666]">Pitch</label>
            <input
              type="range"
              min="-12"
              max="12"
              step="1"
              value={pitchSemitones}
              onChange={(e) => handleSetPitch(Number(e.target.value))}
              className="w-24 h-1 bg-[#444] rounded-lg appearance-none cursor-pointer accent-[#3b82f6]"
            />
          </div>
          <div className="flex flex-col items-center gap-1">
            <label className="text-[10px] text-[#666]">Vol</label>
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              value={volumeLevel * 100}
              onChange={(e) => handleSetVolume(Number(e.target.value) / 100)}
              className="w-24 h-1 bg-[#444] rounded-lg appearance-none cursor-pointer accent-[#22c55e]"
            />
          </div>
        </div>
      </div>

      {/* Hot cues */}
      {cues.length > 0 && (
        <div className="px-3 py-2 border-t border-[#1f1f1f] bg-[#0a0a0a]">
          <div className="flex gap-1 overflow-x-auto pb-1">
            {cues.map((cue, i) => (
              <button
                key={cue.time}
                onClick={() => {
                  // TODO: buscar posição no áudio do deck
                }}
                className={`w-8 h-8 rounded text-xs font-bold flex items-center justify-center transition-colors ${
                  i === 0 ? 'bg-[#3b82f6] text-white' : 'bg-[#333] text-[#ccc] hover:bg-[#444]'
                }`}
                title={`Cue ${i + 1} (${cue.time?.toFixed(2) ?? '0.00'}s)`}
              >
                {i + 1}
              </button>
            ))}
          </div>
          <div className="text-[10px] text-[#666] mt-1">
            {cues.length} cue point(s)
          </div>
        </div>
      )}
    </div>
  );
}
