# OpenDeck UI

Interface visual profissional tipo Rekordbox/Serato construída com **Tauri 2.0 + React 18 + TypeScript + Vite**.

## Estrutura

```
crates/opendeck-ui/
├── Cargo.toml                    # Manifesto Rust (workspace)
├── package.json                  # Dependências frontend
├── vite.config.ts                # Config Vite + rollup-plugin-visualizer
├── tailwind.config.js            # Design tokens TailwindCSS
├── postcss.config.cjs            # Plugins PostCSS
├── tsconfig.json                 # TypeScript strict
├── index.html                    # Entry point HTML
├── src/
│   ├── main.tsx                  # React bootstrap
│   ├── index.css                 # Tailwind + variáveis CSS
│   ├── types.ts                  # Interfaces TS espelhando structs Rust
│   ├── services/api.ts           # Camada de invoke() Tauri
│   ├── components/
│   │   ├── App.tsx               # Layout principal (sidebar + waveform + controles)
│   │   ├── TrackList.tsx         # Lista de tracks clicável
│   │   ├── WaveformCanvas.tsx    # Canvas com gradiente e beatgrid
│   │   └── Controls.tsx          # Play/Pause/Stop, volume, tempo
│   ├── setupTests.ts             # Mocks jsdom + matchers
│   ├── App.test.tsx              # Teste integração frontend↔backend
│   ├── components/
│   │   └── WaveformCanvas.test.tsx # Teste renderização canvas
│   └── services/
│       └── api.test.ts           # Teste unitário comandos Tauri
├── src-tauri/
│   ├── Cargo.toml                # Dependências Rust (tauri + crates opendeck)
│   ├── tauri.conf.json           # Configuração Tauri 2.0
│   ├── build.rs                  # Build script Tauri
│   ├── capabilities/
│   │   └── default.json          # Permissões padrão
│   ├── icons/
│   └── src/
│       ├── lib.rs                # Comandos Tauri + App factory
│       └── main.rs               # Entry point Windows
└── dist/                         # Build de produção (gerado)
    └── bundle-analysis.html      # Análise de bundle (gerado)
```

## Backend Rust (src-tauri/src/lib.rs)

Cinco comandos Tauri que integram os crates existentes:

| Comando | Função Rust | Retorno |
|---------|-------------|---------|
| `load_library` | `RekordboxDb::open(db_path, key)` → `get_tracks()` | `Vec<Track>` |
| `load_track_analysis` | `DatParser::parse_file(dat_path)` | `TrackAnalysis` |
| `play_track` | `AudioEngine::new()` + `load_track()` + `play()` | `()` |
| `stop_track` | `AudioEngine::stop()` | `()` |
| `get_devices` | `UsbScanner::new()` → `scan()` | `Vec<DeviceInfo>` |

## Frontend React

- **App.tsx**: Layout flex com sidebar esquerda (300px), área central (waveform canvas + controles), tema escuro (`#1a1a1a` fundo, `#e0e0e0` texto)
- **TrackList.tsx**: Lista clicável com destaque da track selecionada, BPM e duração
- **WaveformCanvas.tsx**: Canvas com gradiente vermelho→verde→azul, marcadores de beatgrid verticais (PQTZ), linha de posição atual em tempo real
- **Controls.tsx**: Botões Play/Pause/Stop com SVG, slider volume (0-100%), slider tempo (0.5x-2.0x), display tempo decorrido/total
- **types.ts**: Interfaces `Track`, `TrackAnalysis`, `DeviceInfo` espelhando structs Rust

## Testes

- `src/services/api.test.ts` — 6 testes unitários dos comandos Tauri (mock de `window.__TAURI__.invoke`)
- `src/components/WaveformCanvas.test.tsx` — 4 testes de renderização do canvas com dados mock
- `src/App.test.tsx` — 5 testes de integração frontend↔backend via `invoke()`

Rodar: `npm test` (ou `npx vitest run`)

## Comandos de Desenvolvimento

```bash
# 1. Instalar dependências frontend
cd crates/opendeck-ui
npm install

# 2. Build do frontend (obrigatório antes do backend no Tauri 2.0)
npm run build

# 3. Rodar em desenvolvimento
cargo tauri dev

# 4. Build para produção
cargo tauri build

# 5. Executar testes
npm test

# 6. Lint
npm run lint
```

## Variáveis de Ambiente Necessárias (Windows)

O SQLite com SQLCipher requer OpenSSL. Antes de compilar o backend:

```powershell
$env:OPENSSL_DIR="C:\Program Files\OpenSSL-Win64"
$env:OPENSSL_LIB_DIR="C:\Program Files\OpenSSL-Win64\lib\VC\x64\MD"
```

## Troubleshooting

| Erro | Causa | Solução |
|------|-------|---------|
| `Missing environment variable OPENSSL_DIR` | libsqlite3-sys não encontra OpenSSL | Definir `OPENSSL_DIR` e `OPENSSL_LIB_DIR` apontando para a instalação OpenSSL-Win64 |
| `OpenSSL library directory does not exist` | Caminho incorreto para libs estáticas | Usar `C:\Program Files\OpenSSL-Win64\lib\VC\x64\MD` |
| `Failed to load PostCSS config: module is not defined` | `postcss.config.js` tratado como ESM | Renomear para `postcss.config.cjs` |
| `cargo: command not found` | Rust não no PATH | Adicionar `%USERPROFILE%\.cargo\bin` ao PATH |
| `tauri: command not found` | Tauri CLI não instalado | `npm install -g @tauri-apps/cli@latest` |
| Frontend não carrega no `cargo tauri dev` | `dist/` não existe | Rodar `npm run build` primeiro (requisito Tauri 2.0) |
| `No matching version found for oxlint` | Pacote inexistente | Remover `oxlint` do `package.json` |

## Build System

- **Frontend**: Vite 5 com Rollup, code splitting manual (`vendor`, `tauri` chunks), `rollup-plugin-visualizer` gera `bundle-analysis.html`
- **Backend**: Cargo com `tauri-build`, multi-crate workspace, `tauri.conf.json` com `frontendDist: ../dist`, `devUrl: @url:http://localhost:5173`
- **Tauri 2.0**: Requer que o frontend seja buildado **antes** do backend (`beforeBuildCommand: npm run build`)
