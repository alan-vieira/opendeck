import type { Track } from '../types';

interface TrackListProps {
  tracks: Track[];
  isLoading?: boolean;
  selectedTrack: Track | null;
  onSelect: (track: Track) => void;
  onDoubleClick?: (track: Track) => void; // 👈 Adicionado aqui
}

export default function TrackList({
  tracks,
  isLoading = false,
  selectedTrack,
  onSelect,
  onDoubleClick, // 👈 Adicionado aqui
}: TrackListProps) {
  if (isLoading) {
    return (
      <div className="space-y-1">
        {[1, 2, 3, 4, 5].map((i) => (
          <div key={i} className="flex items-center gap-3 px-3 py-2 rounded bg-[#1a1a1a] animate-pulse">
            <div className="w-8 h-8 rounded bg-[#333] shrink-0" />
            <div className="flex-1 min-w-0 space-y-1">
              <div className="h-3 w-3/4 rounded bg-[#333]" />
              <div className="h-2 w-1/2 rounded bg-[#333]" />
            </div>
            <div className="text-right text-xs font-mono text-[#666]">
              <div className="h-3 w-1/3 rounded bg-[#333]" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  return (
    <div className="space-y-1">
      {tracks.map((track) => (
        <div
          key={track.id}
          onClick={() => onSelect(track)}
          onDoubleClick={() => onDoubleClick?.(track)} // 👈 Adicionado aqui
          className={`flex items-center gap-3 px-3 py-2 rounded cursor-pointer transition-colors ${
            selectedTrack?.id === track.id
              ? 'bg-[#3a3a3a] border-l-2 border-[#3b82f6] text-white'
              : 'hover:bg-[#1a1a1a] text-[#c0c0c0]'
          }`}
        >
          <div className="w-8 h-8 rounded bg-[#333] flex items-center justify-center text-xs font-bold text-[#666] shrink-0">
            {track.title.charAt(0).toUpperCase()}
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium truncate">{track.title}</div>
            <div className="text-xs text-[#8a8a8a] truncate">{track.artist}</div>
          </div>
          <div className="text-right text-xs font-mono text-[#888] shrink-0">
            {track.bpm ? `${track.bpm.toFixed(0)} BPM` : '-- BPM'}
          </div>
        </div>
      ))}
      {tracks.length === 0 && (
        <div className="text-center py-8 text-[#666] text-sm">
          Nenhuma track encontrada. Carregue a biblioteca.
        </div>
      )}
    </div>
  );
}