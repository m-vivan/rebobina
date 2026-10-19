# Gravador

App macOS para gravar reuniões localmente, transcrever e resumir com IA (OpenAI Whisper + GPT).

Captura **microfone + áudio do sistema** via ScreenCaptureKit — sem bot na call e sem BlackHole.

## Identidade

- **Nome:** Gravador  
- **Marca:** ponto coral de gravação + barras de onda em ciano  
- **Paleta:** fundo `#0C1220`, coral `#FF6B4A`, ciano `#5CC8FF`  
- **Tipografia:** Syne (títulos) · Figtree (texto)

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

- `apps/desktop/src-tauri/target/release/bundle/dmg/Gravador_*.dmg`
- `apps/desktop/src-tauri/target/release/bundle/macos/Gravador.app`

Abra o `.dmg` e arraste **Gravador** para Aplicativos. Na primeira abertura (app não assinado): clique direito → Abrir. Autorize **Gravação de tela**, **Microfone** e **Notificações**.

Na raiz: `npm run build`.

## Uso

1. Autorize Gravação de tela + microfone
2. Configure a API key em **Configurações**
3. Inicie a gravação (ou aceite a notificação ao detectar uma call)
4. Pare — o app transcreve (Whisper) e resume (GPT-4o)

## Dados locais

`~/Library/Application Support/gravador/`

## Licença

MIT
