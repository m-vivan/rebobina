# Setup de áudio no macOS (ScreenCaptureKit)

O Rebobina captura a reunião em duas faixas:

1. **Áudio do sistema** (outra pessoa no Zoom/Meet/Teams) via **ScreenCaptureKit**
2. **Microfone** (sua voz) via API de microfone do macOS

Não é necessário instalar BlackHole nem criar Aggregate Device.

## 1. Permissão de Gravação de tela

Na primeira gravação o macOS pede permissão. Isso é normal: a Apple empacota a
captura de áudio do sistema dentro da API de tela. O Rebobina **não salva vídeo**
(usa um frame 2×2 descartado).

Se negar ou o diálogo não aparecer:

1. **Ajustes do Sistema → Privacidade e segurança → Gravação de tela**
2. Ative o **Rebobina**
3. Reinicie o app

No próprio Rebobina: **Configurações → Permitir captura** ou **Abrir Ajustes do macOS**.

## 2. Microfone

Autorize o microfone quando o sistema pedir. Em Configurações você escolhe qual
microfone usar (AirPods, embutido, etc.).

## 3. Checklist antes de gravar

- [ ] Gravação de tela liberada para o Rebobina
- [ ] Microfone liberado
- [ ] Você ouve a reunião nos fones/alto-falantes (o ScreenCaptureKit captura essa saída)
- [ ] API key OpenAI configurada (para transcrever/resumir)

## Problemas comuns

| Sintoma | Correção |
|--------|----------|
| Só sua voz | Verifique Gravação de tela; reinicie o app após liberar |
| Erro “recusou os TCCs” com toggle **ligado** | 1) **Cmd+Q** (fechar de verdade) 2) em Ajustes desligar e religar Rebobina 3) abrir de `/Aplicativos`. Se o DMG for **ad-hoc** (release sem certificado Apple), no **macOS 15+** o ScreenCaptureKit pode continuar recusando — precisa build assinado (Developer ID). |
| Erro ao iniciar captura | Ajustes → Gravação de tela → Rebobina ON → **Cmd+Q** → abrir de novo |
| VU mudo | Aumente o volume da reunião; confira se há áudio saindo do Mac |
| Eco | Use fones |
| Whisper 413 Payload Too Large | Áudio > ~25 MB; o app fatia automaticamente. Atualize o Rebobina e clique em **Tentar de novo**. |

### Reset manual da permissão

```bash
tccutil reset ScreenCapture dev.vivan.rebobina
```

Depois abra o Rebobina, conceda de novo, **Cmd+Q** e abra outra vez.

### Assinatura ad-hoc (releases sem `APPLE_CERTIFICATE`)

No macOS Sequoia (15) e posteriores, o TCC de Gravação de tela / ScreenCaptureKit
exige identidade de código com Team ID Apple. Builds com `signingIdentity: "-"`
podem aparecer como **ligados** em Ajustes e mesmo assim falhar na captura.
Para uso real, configure os secrets Apple no GitHub Actions e publique uma
release assinada.

## BlackHole (opcional / legado)

BlackHole não é mais necessário. O código de instalação antiga permanece só como
fallback avançado; o fluxo padrão é ScreenCaptureKit.
