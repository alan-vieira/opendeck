export default function Mixer() {
  return (
    <div className="h-24 bg-[#111111] border-t border-[#1f1f1f] flex items-center justify-between px-6">
      {/* Crossfader */}
      <div className="flex-1 max-w-96 flex flex-col items-center gap-2">
        <label className="text-[10px] text-[#666] uppercase tracking-wider">Crossfader</label>
        <div className="w-full h-4 bg-[#333] rounded-lg relative overflow-hidden">
          <div
            className="absolute left-1/2 top-0 bottom-0 w-2 bg-[#3b82f6] transform -translate-x-1/2"
          />
        </div>
      </div>

      {/* EQ / Controles centrais */}
      <div className="flex items-center gap-8">
        <div className="flex flex-col items-center gap-2">
          <label className="text-[10px] text-[#666]">EQ Low</label>
          <div className="w-16 h-1 bg-[#444] rounded-lg relative">
            <div className="absolute left-1/2 top-0 bottom-0 w-1 bg-[#f59e0b] transform -translate-x-1/2" />
          </div>
        </div>
        <div className="flex flex-col items-center gap-2">
          <label className="text-[10px] text-[#666]">EQ Mid</label>
          <div className="w-16 h-1 bg-[#444] rounded-lg relative">
            <div className="absolute left-1/2 top-0 bottom-0 w-1 bg-[#f59e0b] transform -translate-x-1/2" />
          </div>
        </div>
        <div className="flex flex-col items-center gap-2">
          <label className="text-[10px] text-[#666]">EQ High</label>
          <div className="w-16 h-1 bg-[#444] rounded-lg relative">
            <div className="absolute left-1/2 top-0 bottom-0 w-1 bg-[#f59e0b] transform -translate-x-1/2" />
          </div>
        </div>
      </div>

      {/* Indicadores */}
      <div className="flex items-center gap-4 text-[#8a8a8a] text-xs font-mono">
        <div className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full bg-[#22c55e]" /> Master
        </div>
        <div className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full bg-[#3b82f6]" /> Deck 1
        </div>
        <div className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full bg-[#f59e0b]" /> Deck 2
        </div>
      </div>
    </div>
  );
}
