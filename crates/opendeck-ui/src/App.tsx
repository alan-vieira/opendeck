import { useState, useCallback, useEffect } from 'react';
import TrackList from './components/TrackList';
import Deck from './components/Deck';
import Mixer from './components/Mixer';
import AuthModal from './components/AuthModal';
import { loadLibrary, logout } from './services/api';
import type { Track } from './types';
import { AuthProvider, useAuth } from './contexts/AuthContext';

function AppInner() {
  const { isAuthenticated, login, logout: authLogout } = useAuth();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [selectedTrack, setSelectedTrack] = useState<Track | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [dbError, setDbError] = useState<string | null>(null);
  const [authOpen, setAuthOpen] = useState(!isAuthenticated);

  useEffect(() => {
    if (!isAuthenticated) return;
    setIsLoading(true);
    setDbError(null);
    loadLibrary('C:/Users/alsvi/AppData/Roaming/Pioneer/rekordbox/master.db')
      .then((loadedTracks) => {
        setTracks(loadedTracks);
      })
      .catch((err) => {
        const msg = err instanceof Error ? err.message : String(err);
        setDbError(msg);
      })
      .finally(() => setIsLoading(false));
  }, [isAuthenticated]);

  const handleTrackSelect = useCallback((track: Track) => {
    setSelectedTrack(track);
  }, []);

  const handleLogin = useCallback((token: string) => {
    login(token);
    setAuthOpen(false);
  }, [login]);

  const handleLogout = useCallback(() => {
    authLogout();
    logout();
    setTracks([]);
    setSelectedTrack(null);
    setAuthOpen(true);
  }, [authLogout]);

  return (
    <div className="flex h-screen bg-[#0a0a0a] text-[#fafafa] overflow-hidden">
      {/* Auth Modal */}
      {authOpen && (
        <AuthModal onLogin={handleLogin} onClose={() => setAuthOpen(false)} />
      )}

      {/* Biblioteca lateral */}
      <aside className="w-80 bg-[#111111] border-r border-[#1f1f1f] flex flex-col">
        <div className="p-4 border-b border-[#1f1f1f]">
          <h1 className="text-xl font-bold text-white flex items-center gap-2">
            <span className="text-[#3b82f6]">🎧</span> OpenDeck
          </h1>
          <p className="text-xs text-[#8a8a8a] mt-1">Biblioteca de Tracks</p>
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          {dbError && (
            <div className="p-3 bg-[#ef4444]/10 border border-[#ef4444]/30 rounded text-[#ef4444] text-sm mb-2">
              Erro ao carregar biblioteca: {dbError}
            </div>
          )}
          <TrackList
            tracks={tracks}
            isLoading={isLoading}
            selectedTrack={selectedTrack}
            onSelect={handleTrackSelect}
          />
        </div>
        <div className="p-3 border-t border-[#1f1f1f] bg-[#111111]">
          <button
            onClick={handleLogout}
            className="w-full py-2 text-sm text-[#ef4444] hover:bg-[#ef4444]/10 rounded transition-colors"
          >
            Sair
          </button>
        </div>
      </aside>

      <main className="flex-1 flex flex-col relative bg-[#0a0a0a]">
        <div className="absolute inset-0 z-0 bg-[#1a1a1a] opacity-10 pointer-events-none"></div>

        <div className="relative z-10 flex-1 flex flex-col p-4">
          {/* Duas waveforms lado a lado */}
          <div className="flex-1 mb-4 flex gap-4">
            <Deck
              deckId={1}
              track={selectedTrack}
              onLoadTrack={(track: Track) => {
                setSelectedTrack(track);
              }}
            />
            <Deck
              deckId={2}
              track={selectedTrack}
              onLoadTrack={(track: Track) => {
                setSelectedTrack(track);
              }}
            />
          </div>

          {/* Mixer central */}
          <Mixer />
        </div>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <AuthProvider>
      <AppInner />
    </AuthProvider>
  );
}
