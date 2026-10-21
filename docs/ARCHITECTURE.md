# OpenDeck - Arquitetura do Sistema

## Visão Geral
OpenDeck é um software de DJ compatível com a biblioteca Rekordbox 6.x, desenvolvido em Rust com frontend Tauri. O projeto segue o princípio *Clean Room*, focando em interoperabilidade sem copiar código proprietário. O backend utiliza o runtime `tokio` para garantir operações de I/O não bloqueantes, essenciais para a estabilidade do áudio e da interface.

## Estrutura de Crates (Workspace)
1. **`opendeck-core`**: Modelos de dados (`Track`, `CuePoint`, `TrackAnalysis`), Traits de repositório e tipos de erro (`thiserror`). Não depende de nenhum outro crate.
2. **`opendeck-analysis`**: Parser binário de alta performance para arquivos `track_analysis.DAT` (usa `memmap2` e `byteorder`).
3. **`opendeck-db`**: Leitor do `master.db` utilizando `rusqlite` com a feature `bundled-sqlcipher`.
4. **`opendeck-audio`**: Motor de reprodução de baixa latência usando `cpal` (ASIO/WASAPI no Windows, ALSA/PipeWire no Linux).
5. **`opendeck-hardware`**: Detecção de dispositivos Pioneer (`VID_09E8`) via `rusb` e mapeamento de comandos MIDI via `midir`.
6. **`opendeck-ui`**: Aplicativo Tauri (React + TypeScript) que consome os crates locais via comandos Rust (`#[tauri::command]`).

## Fluxo de Dados (Data Flow)

1. **Inicialização**: O `opendeck-ui` solicita a lista de faixas ao backend.
2. **Leitura**: O `opendeck-db` descriptografa e consulta o `master.db`, retornando structs do `opendeck-core`.
3. **Análise**: Ao carregar uma faixa no Deck, o `opendeck-audio` solicita ao `opendeck-analysis` os dados de beatgrid e waveform via mmap (zero-copy).
4. **Reprodução**: O `opendeck-audio` alimenta o buffer de saída do `cpal` em uma thread de alta prioridade, enquanto o estado de reprodução é sincronizado com a UI via eventos do Tauri.

## Princípios de Design
- **Cross-Platform First**: Uso de `#[cfg(target_os = "...")]` para comportamentos específicos de SO.
- **Zero-Copy Parsing**: Uso de `memmap2` para ler arquivos `.DAT` grandes sem alocar memória desnecessária.
- **Graceful Degradation**: Se um arquivo `.DAT` estiver corrompido ou ausente, o sistema ignora a análise (waveform/beatgrid), mas mantém os metadados do banco de dados funcionais.