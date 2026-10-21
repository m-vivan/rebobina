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

## Publicar uma release

O CD (`.github/workflows/release.yml`) gera o DMG universal macOS e anexa a uma [GitHub Release](../../releases).

```bash
# 1) Ajuste a versão em apps/desktop/src-tauri/tauri.conf.json
# 2) Tag + push
git checkout main && git pull
git tag v0.1.0
git push origin v0.1.0
```

Alternativa: Actions → **Release** → **Run workflow** (informe a tag, ex. `v0.1.0`).

Artefatos tipicamente publicados: `Rebobina_<versão>_*.dmg` (e metadados do bundle Tauri).

### Assinatura / notarização

Sem secrets o build ainda publica o DMG com assinatura **ad-hoc** (`signingIdentity: "-"`).
Isso basta para instalar e abrir (clique direito → Abrir), mas no **macOS 15+** o
ScreenCaptureKit / Gravação de tela **costuma recusar** apps ad-hoc mesmo com o
toggle ligado em Ajustes. Para gravar reuniões de verdade, configure o certificado:

| Secret | Função |
|--------|--------|
| `APPLE_CERTIFICATE` | `.p12` em base64 (**Developer ID Application** — obrigatório para SCK em produção) |
| `APPLE_CERTIFICATE_PASSWORD` | Senha do `.p12` |
| `KEYCHAIN_PASSWORD` | Senha da keychain temporária no CI |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | Notarização via Apple ID (recomendado) |
| `APPLE_API_ISSUER` / `APPLE_API_KEY` / `APPLE_API_KEY_PATH` | Notarização via App Store Connect API |

#### Atalho: script de configuração

Com o certificado já no chaveiro (passos 1 a 3 abaixo), o script exporta só a
identidade de assinatura, confere o arquivo e cadastra os secrets
`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` e `KEYCHAIN_PASSWORD`:

```bash
scripts/configurar-assinatura.sh            # cadastra no GitHub
scripts/configurar-assinatura.sh --dry-run  # só gera e confere, sem enviar
```

Funciona com **Apple Development** (gratuito; release assinada, mas não
notarizada) e com **Developer ID** (`IDENTITY_PREFIX="Developer ID Application"`).
Se o certificado aparecer como inválido, instale a intermediária
[WWDR G3](https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer).

#### Como obter o certificado e configurar

1. Entre no [Apple Developer Program](https://developer.apple.com/programs/) (pago, anual). Conta pessoal serve.
2. No Xcode → **Settings → Accounts**, selecione a conta → **Manage Certificates** → **+** → **Developer ID Application**.
3. No app **Acesso às Chaves**, em *Meus Certificados*, exporte o certificado "Developer ID Application: …" (com a chave privada) como `.p12` e defina uma senha.
4. Gere o base64 e copie:

   ```bash
   base64 -i certificado.p12 | pbcopy
   ```

5. Em [appleid.apple.com](https://appleid.apple.com) → **Senhas de app**, crie uma senha para a notarização.
6. Em **Settings → Secrets and variables → Actions** do repositório, crie:
   `APPLE_CERTIFICATE` (o base64), `APPLE_CERTIFICATE_PASSWORD` (senha do `.p12`),
   `KEYCHAIN_PASSWORD` (qualquer senha), `APPLE_ID` (seu e-mail da Apple),
   `APPLE_PASSWORD` (a senha de app) e `APPLE_TEAM_ID` (em developer.apple.com → Membership).
7. Publique uma nova tag. O workflow detecta o certificado, assina com a identidade Developer ID, notariza e publica o DMG.

Com o app assinado, a permissão de Gravação de tela passa a valer entre versões
(no ad-hoc ela fica presa ao hash de cada build e precisa ser liberada de novo).

#### Entitlements

O build usa **Hardened Runtime** (exigido pela notarização) com
[`Entitlements.plist`](apps/desktop/src-tauri/Entitlements.plist). Ele declara
`com.apple.security.device.audio-input`: sem isso o macOS entrega silêncio no
microfone de um app com Hardened Runtime, sem mostrar o pedido de acesso.

### Atualização automática (dentro do app)

O app procura versões novas no `latest.json` da última release e mostra
"Nova versão do Rebobina" no topo. O botão **Atualizar e reiniciar** baixa
e instala a versão nova pelo próprio app; esse download não recebe a marca
de quarentena do macOS, então não aparece o "Abrir Mesmo Assim".

Cada atualização é assinada com uma chave própria (diferente do certificado
Apple). Para configurar uma vez:

```bash
scripts/configurar-atualizador.sh
```

O script gera o par de chaves em `~/.tauri/rebobina-updater.key`, cadastra
`TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` e grava a
chave **pública** em `plugins.updater.pubkey` do `tauri.conf.json` (faça
commit dela). **Guarde a chave privada e a senha:** sem elas não dá para
publicar atualizações que as versões instaladas aceitem.

No CI, com a chave presente, o build usa `src-tauri/tauri.updater.conf.json`
para gerar o `.app.tar.gz`, a assinatura `.sig` e o `latest.json`. Builds
locais continuam sem exigir a chave.

## Licença

Ao contribuir, você concorda que suas contribuições serão licenciadas sob a [MIT License](LICENSE) do projeto (Copyright Marivaldo Vivan).
