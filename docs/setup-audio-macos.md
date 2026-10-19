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
| Erro ao iniciar captura | Ajustes → Gravação de tela → Rebobina ON → reinicie |
| VU mudo | Aumente o volume da reunião; confira se há áudio saindo do Mac |
| Eco | Use fones |

## BlackHole (opcional / legado)

BlackHole não é mais necessário. O código de instalação antiga permanece só como
fallback avançado; o fluxo padrão é ScreenCaptureKit.
