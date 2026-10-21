# 🎧 OpenDeck

Software de DJ open-source compatível com a biblioteca Rekordbox 6.x, focado em performance, baixa latência e portabilidade (Windows/Linux).

## 🏗️ Arquitetura

O projeto é um Workspace Rust dividido em crates modulares:
- `opendeck-core`: Modelos de dados e traits compartilhadas.
- `opendeck-db`: Leitor do `master.db` criptografado (SQLCipher).
- `opendeck-analysis`: Parser binário zero-copy dos arquivos `.DAT` (Waveform/Beatgrid).
- `opendeck-audio`: Motor de áudio de baixa latência (`cpal` + `symphonia`).
- `opendeck-hardware`: Detecção USB e mapeamento MIDI de controladores Pioneer.
- `opendeck-ui`: Interface gráfica moderna construída com Tauri v2, React e Tailwind CSS.

## 🚀 Como Rodar

### Pré-requisitos
- [Rust](https://rustup.rs/) (toolchain estável)
- [Node.js](https://nodejs.org/) (v18+)
- Tauri CLI: `cargo install tauri-cli --version "^2.0.0"`
- **Windows:** NASM (necessário para compilar o OpenSSL vendored): `winget install nasm`

### Instalação e Execução

1. Clone o repositório:
   ```bash
   git clone https://github.com/SEU_USUARIO/opendeck.git
   cd opendeck
   ```

2. Instale as dependências do frontend:
   ```bash
   cd crates/opendeck-ui
   npm install
   ```

3. Inicie o ambiente de desenvolvimento:
   ```bash
   cargo tauri dev
   ```

## 🛠️ Status do Desenvolvimento

- [x] Parser de arquivos .DAT (PQTZ, PWAV)
- [x] Leitura do master.db com SQLCipher
- [x] Motor de áudio com buffer de 128 frames (~2.67ms)
- [x] Interface Tauri + React funcional
- [ ] Integração completa Deck A / Deck B + Mixer
- [ ] Suporte a controladores MIDI (DDJ-400/FLX4)

*(Lembre-se de trocar `SEU_USUARIO` pelo seu nome de usuário real do GitHub.)*