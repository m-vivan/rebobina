# Rebobina

App macOS para gravar reuniões localmente, transcrever e resumir com IA (OpenAI Whisper + GPT).

Captura **microfone + áudio do sistema** via ScreenCaptureKit — sem bot na call e sem BlackHole.

## Identidade

- **Nome:** Rebobina (antes Gravador). Grava a reunião e depois deixa você voltar ao que importa, como rebobinar uma fita K7.
- **Marca:** fita cassete com o carretel esquerdo em vermelho REC e três listras de etiqueta
- **Paleta:** Fita `#1E1714`, Casco `#2A211C`, Etiqueta `#EADFC6`, REC `#E8402A`, Âmbar `#F2A23A`, Petróleo `#2F8F87`
- **Tipografia:** Archivo Expanded Black (títulos) · Figtree (texto) · VT323 (contador) · Caveat (títulos das fitas)
- **Arquivos:** [`brand/`](brand/) — símbolo, ícone e guia de identidade (`rebobina-identidade.html`)

Os identificadores internos continuam `gravador` (bundle `com.vivan.gravador`, pasta de dados, keychain), para manter as gravações e permissões já existentes.

## Requisitos

- macOS 13+ (recomendado 15+ para mic via ScreenCaptureKit)
- Node.js 20+
- Rust (stable) — [rustup](https://rustup.rs)
- Xcode.app (ScreenCaptureKit / Swift)
- API key OpenAI

## Como captura o áudio

- Áudio remoto → ScreenCaptureKit (permissão de **Gravação de tela**; sem vídeo salvo)
- Sua voz → microfone

Detalhes: [docs/setup-audio-macos.md](docs/setup-audio-macos.md).

## Desenvolvimento

```bash
cd apps/desktop
npm install
npm run tauri:dev
```

Se o build falhar com erros de Swift:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

## Build / instalador (DMG)

```bash
cd apps/desktop
npm install
npm run tauri:build
```

Artefatos:

- `apps/desktop/src-tauri/target/release/bundle/dmg/Rebobina_*.dmg`
- `apps/desktop/src-tauri/target/release/bundle/macos/Rebobina.app`

Abra o `.dmg` e arraste **Rebobina** para Aplicativos. Na primeira abertura (app não assinado): clique direito → Abrir. Autorize **Gravação de tela**, **Microfone** e **Notificações**.

Na raiz: `npm run build`.

## Uso

1. Autorize Gravação de tela + microfone
2. Configure a API key em **Configurações**
3. Inicie a gravação (ou aceite a notificação ao detectar uma call)
4. Pare e rebobine — o app transcreve (Lado A, Whisper) e resume (Lado B, GPT-4o)

## Dados locais

`~/Library/Application Support/gravador/`

## Licença

MIT
