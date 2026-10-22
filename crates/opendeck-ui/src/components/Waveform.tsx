import { useRef, useEffect, useLayoutEffect } from 'react';
import type { Track, CuePoint, TrackAnalysis } from '../types';

interface WaveformProps {
  track: Track | null;
  analysis: TrackAnalysis | null;
  cues: CuePoint[];
  currentTime: number;
  duration: number;
  accentColor: string;
}

export default function Waveform({
  track,
  analysis,
  cues,
  currentTime,
  duration,
  accentColor,
}: WaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const sizeRef = useRef({ width: 0, height: 0 });

  // Debug da duração
  useEffect(() => {
    if (track && duration > 0) {
      console.log(`🎵 [WAVEFORM] Track: ${track.title}`);
      console.log(`⏱️ [WAVEFORM] Duration: ${duration}s, Current: ${currentTime}s`);
      console.log(`📊 [WAVEFORM] Waveform pixels: ${analysis?.waveform_amplitudes.length || 0}`);
    }
  }, [track, duration, currentTime, analysis]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const updateSize = () => {
      const rect = container.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      sizeRef.current = { width: rect.width, height: rect.height };
      canvas.width = Math.max(1, Math.floor(rect.width * dpr));
      canvas.height = Math.max(1, Math.floor(rect.height * dpr));
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
    };

    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      console.error('❌ [WAVEFORM] Canvas não encontrado');
      return;
    }

    const ctx = canvas.getContext('2d');
    if (!ctx) {
      console.error('❌ [WAVEFORM] Contexto 2D não encontrado');
      return;
    }

    const { width, height } = sizeRef.current;
    if (width <= 0 || height <= 0) return;

    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    // Fundo SEMPRE visível (cinza escuro)
    ctx.fillStyle = '#1a1a1a';
    ctx.fillRect(0, 0, width, height);

    // Caso 1: Sem análise
    if (!analysis) {
      ctx.fillStyle = '#666';
      ctx.font = '14px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('Analisando áudio...', width / 2, height / 2);
      return;
    }

    const waveform = analysis.waveform_amplitudes;
    if (!waveform || waveform.length === 0) {
      ctx.fillStyle = '#f00';
      ctx.font = '14px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('Erro: Sem dados de waveform', width / 2, height / 2);
      console.error('❌ [WAVEFORM] waveform_amplitudes está vazio!');
      return;
    }

    console.log('✅ [WAVEFORM] Desenhando', waveform.length, 'pixels');

    const centerY = height / 2;
    const maxAmp = height * 0.4; // Um pouco menor para não tocar nas bordas
    const barWidth = Math.max(2, width / waveform.length); // Mínimo 2px para ficar visível
    const progressRatio = duration > 0 ? Math.min(1, currentTime / duration) : 0;

    let maxAmplitudeFound = 0;

    // Desenha cada barra - AGORA VISÍVEL
    waveform.forEach((amp, i) => {
      const normalized = Math.min(1, Math.max(0, amp / 255));
      const barHeight = Math.max(1, normalized * maxAmp); // Mínimo 1px
      const x = i * barWidth;
      const barRatio = i / waveform.length;

      if (barRatio <= progressRatio) {
        // Já tocou - COR FORTE E VISÍVEL
        ctx.fillStyle = accentColor === '#3b82f6' ? '#60a5fa' : '#fb923c';
      } else {
        // Ainda não tocou - CINZA CLARO VISÍVEL
        ctx.fillStyle = 'rgba(180, 180, 180, 0.8)';
      }

      ctx.fillRect(x, centerY - barHeight, Math.max(1, barWidth - 1), barHeight * 2);
      if (amp > maxAmplitudeFound) maxAmplitudeFound = amp;
    });

    console.log(`📊 [WAVEFORM] ${waveform.length} barras desenhadas, maxAmp: ${maxAmplitudeFound}/255`);

    // Playhead
    if (duration > 0) {
      const progressX = progressRatio * width;
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(progressX, 0);
      ctx.lineTo(progressX, height);
      ctx.stroke();

      ctx.fillStyle = '#fff';
      ctx.beginPath();
      ctx.arc(progressX, 6, 4, 0, Math.PI * 2);
      ctx.fill();
    }

    // Cues
    cues.forEach((cue) => {
      if (duration <= 0) return;
      const cueX = (cue.start_ms / 1000 / duration) * width;
      if (cueX < 0 || cueX > width) return;

      const color = cue.color_rgb
        ? `rgb(${cue.color_rgb.join(',')})`
        : accentColor;

      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 4]);
      ctx.beginPath();
      ctx.moveTo(cueX, 0);
      ctx.lineTo(cueX, height);
      ctx.stroke();
      ctx.setLineDash([]);

      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.moveTo(cueX, 0);
      ctx.lineTo(cueX - 5, 8);
      ctx.lineTo(cueX + 5, 8);
      ctx.closePath();
      ctx.fill();
    });
  }, [analysis, cues, currentTime, duration, accentColor, track]);

  return (
    <div
      ref={containerRef}
      className="relative w-full h-32 bg-[#1a1a1a] rounded border border-[#333] overflow-hidden"
    >
      <canvas
        ref={canvasRef}
        className="block w-full h-full"
        aria-label="Waveform da track"
      />
      {cues.length > 0 && (
        <div className="absolute bottom-1 left-2 right-2 flex gap-1 justify-center pointer-events-none">
          {cues.slice(0, 8).map((cue) => (
            <span
              key={cue.id}
              className="text-[10px] px-1 rounded bg-black/70 text-white"
              style={{ 
                color: cue.color_rgb ? `rgb(${cue.color_rgb.join(',')})` : accentColor 
              }}
            >
              {cue.hot_cue_index !== null ? String.fromCharCode(65 + cue.hot_cue_index) : 'M'}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}