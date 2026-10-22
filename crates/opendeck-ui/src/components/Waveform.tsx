import { useRef, useEffect } from 'react';
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

  // Desenha o waveform no canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !analysis) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const { width, height } = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.scale(dpr, dpr);

    // Limpa
    ctx.clearRect(0, 0, width, height);

    const waveform = analysis.waveform_amplitudes;
    if (!waveform || waveform.length === 0) return;

    const centerY = height / 2;
    const maxAmp = height * 0.45;
    const barWidth = Math.max(1, width / waveform.length);

    // Desenha barras do waveform
    waveform.forEach((amp, i) => {
      const normalized = amp / 255; // 0-255
      const barHeight = normalized * maxAmp;
      const x = i * barWidth;

      // Gradiente baseado na posição (beatgrid)
      const isBeat = analysis.beatgrid_seconds.some(
        (beat) => Math.abs((i / waveform.length) * duration - beat) < 0.1
      );

      if (isBeat) {
        ctx.fillStyle = accentColor;
      } else {
        ctx.fillStyle = 'rgba(200, 200, 200, 0.6)';
      }

      ctx.fillRect(x, centerY - barHeight, barWidth, barHeight * 2);
    });

    // Desenha posição atual (playhead)
    if (duration > 0) {
      const progressX = (currentTime / duration) * width;
      ctx.strokeStyle = accentColor;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(progressX, 0);
      ctx.lineTo(progressX, height);
      ctx.stroke();

      // Círculo no playhead
      ctx.fillStyle = accentColor;
      ctx.beginPath();
      ctx.arc(progressX, centerY, 5, 0, Math.PI * 2);
      ctx.fill();
    }

    // Desenha cues
    cues.forEach((cue) => {
      const cueX = (cue.start_ms / 1000 / duration) * width;
      if (cueX < 0 || cueX > width) return;

      const color = cue.color_rgb
        ? `rgb(${cue.color_rgb.join(',')})`
        : accentColor;

      // Linha do cue
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.setLineDash([5, 5]);
      ctx.beginPath();
      ctx.moveTo(cueX, 0);
      ctx.lineTo(cueX, height);
      ctx.stroke();
      ctx.setLineDash([]);

      // Triângulo no topo
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.moveTo(cueX, 0);
      ctx.lineTo(cueX - 5, 10);
      ctx.lineTo(cueX + 5, 10);
      ctx.closePath();
      ctx.fill();
    });
  }, [analysis, cues, currentTime, duration, accentColor, track]);

  return (
    <div className="relative w-full h-32 bg-[#0a0a0a] rounded border border-[#1f1f1f] overflow-hidden">
      <canvas
        ref={canvasRef}
        className="w-full h-full"
        aria-label="Waveform da track"
      />
      {/* Legenda dos cues */}
      {cues.length > 0 && (
        <div className="absolute bottom-1 left-2 right-2 flex gap-1 justify-center pointer-events-none">
          {cues.slice(0, 8).map((cue) => (
            <span
              key={cue.id}
              className="text-[10px] px-1 rounded bg-black/70 text-white"
              style={{ color: cue.color_rgb ? `rgb(${cue.color_rgb.join(',')})` : accentColor }}
            >
              {cue.hot_cue_index !== null ? String.fromCharCode(65 + cue.hot_cue_index) : 'M'}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}