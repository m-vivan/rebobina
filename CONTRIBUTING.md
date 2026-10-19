# Contribuindo para o Rebobina

Obrigado pelo interesse em contribuir. Este guia cobre o fluxo básico para propor mudanças.

## Pré-requisitos

- macOS 13+ (o app usa ScreenCaptureKit)
- Node.js 20+
- Rust (stable) via [rustup](https://rustup.rs)
- Xcode.app (não só Command Line Tools)
- API key OpenAI (para testar transcrição/resumo)

Detalhes de áudio: [docs/setup-audio-macos.md](docs/setup-audio-macos.md).

## Fork, clone e setup

1. Faça um fork do repositório no GitHub.
2. Clone o seu fork:

   ```bash
   git clone https://github.com/<seu-usuario>/rebobina.git
   cd rebobina
   ```

3. Instale as dependências do app desktop:

   ```bash
   cd apps/desktop
   npm install
   ```

4. Suba o app em modo desenvolvimento:

   ```bash
   npm run tauri:dev
   ```

   Se o build falhar com erros de Swift:

   ```bash
   sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
   ```

Na raiz do monorepo também funcionam `npm run dev` / `npm run build` (delegam para `apps/desktop`).

## Branch e commits

1. Crie uma branch a partir de `main`:

   ```bash
   git checkout main
   git pull origin main
   git checkout -b feature/minha-mudanca
   ```

2. Faça commits pequenos e com mensagens claras (o quê e por quê).
3. Prefira uma mudança por PR: correção, feature ou docs — não misture.

## Estilo de código e testes

- **Frontend** (`apps/desktop`): TypeScript + React + Vite. Mantenha tipagem estrita; o CI roda `tsc && vite build`.
- **Backend** (`apps/desktop/src-tauri`): Rust (Tauri 2). Siga o estilo padrão do `rustfmt` quando possível.
- **Testes**: não há suite de testes JS ainda. Há testes unitários em Rust (`cargo test` em `src-tauri`). Inclua ou atualize testes quando a mudança tocar lógica testável (mix de áudio, pipeline, etc.).
- **Lint**: não há script de lint configurado no momento; evite warnings óbvios e mantenha o TypeScript passando.

Antes de abrir o PR, confira localmente:

```bash
# Frontend
cd apps/desktop && npm run build

# Rust (com Xcode selecionado)
cd apps/desktop/src-tauri && cargo test
```

## Pull request

1. Envie a branch para o seu fork e abra um PR contra `main` deste repositório.
2. Descreva o problema, a solução e como testou.
3. Aguarde o CI (build do frontend + `cargo test` no macOS). Corrija falhas se aparecerem.
4. Um mantenedor revisará e fará o merge quando estiver ok.

## Licença

Ao contribuir, você concorda que suas contribuições serão licenciadas sob a [MIT License](LICENSE) do projeto (Copyright Marivaldo Vivan).
