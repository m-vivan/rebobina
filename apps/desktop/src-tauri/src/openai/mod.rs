use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// Tempo máximo de uma transcrição: upload de ~19 MB mais o processamento de
/// até 10 minutos de áudio. O padrão do reqwest (30 s) não é suficiente.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SUMMARY_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Esperas entre tentativas em falhas passageiras (rede, 429, 5xx).
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(3), Duration::from_secs(10)];

fn http_client(timeout: Duration) -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
        .map_err(|e| AppError::Message(format!("Falha ao preparar a conexão: {e}")))
}

/// Envia a requisição e repete em falhas passageiras. Devolve status e corpo.
fn send_with_retry(
    service: &str,
    mut build: impl FnMut() -> AppResult<reqwest::blocking::RequestBuilder>,
) -> AppResult<(reqwest::StatusCode, String)> {
    let mut attempt = 0;
    loop {
        let outcome = build()?.send().and_then(|r| {
            let status = r.status();
            r.text().map(|body| (status, body))
        });
        let retryable = match &outcome {
            Ok((status, _)) => status.as_u16() == 429 || status.is_server_error(),
            // Timeout não entra: repetir só multiplicaria a espera.
            Err(e) => !e.is_timeout() && (e.is_connect() || e.is_request() || e.is_body()),
        };
        if retryable && attempt < RETRY_DELAYS.len() {
            thread::sleep(RETRY_DELAYS[attempt]);
            attempt += 1;
            continue;
        }
        return outcome.map_err(|e| network_error(service, &e));
    }
}

fn network_error(service: &str, e: &reqwest::Error) -> AppError {
    let reason = if e.is_timeout() {
        "a OpenAI demorou demais para responder".to_string()
    } else if e.is_connect() {
        "não foi possível conectar à OpenAI. Verifique sua internet".to_string()
    } else {
        let mut detail = e.to_string();
        let mut source = std::error::Error::source(e);
        while let Some(inner) = source {
            detail.push_str(": ");
            detail.push_str(&inner.to_string());
            source = inner.source();
        }
        detail
    };
    AppError::Message(format!("{service} falhou: {reason}. Tente de novo."))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// "me" (microfone) ou "others" (áudio da chamada), quando dá para saber.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub assignee: Option<String>,
    pub task: String,
    pub due: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredSummary {
    /// Nome curto sugerido para a reunião.
    #[serde(default)]
    pub suggested_title: String,
    pub overview: String,
    #[serde(default)]
    pub participants: Vec<Participant>,
    #[serde(default)]
    pub key_points: Vec<KeyPoint>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub notable_quotes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPoint {
    pub topic: String,
    #[serde(default)]
    pub details: Vec<String>,
}

/// O que se sabe da reunião além da transcrição, para orientar o resumo.
pub struct MeetingContext<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub duration_min: Option<i64>,
    /// Vocabulário das Configurações (nomes, empresas, termos).
    pub vocabulary: &'a str,
}

#[derive(Debug, Deserialize)]
struct WhisperVerbose {
    #[serde(default)]
    segments: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    start: f64,
    end: f64,
    text: String,
    #[serde(default)]
    no_speech_prob: f64,
    #[serde(default)]
    avg_logprob: f64,
    #[serde(default)]
    compression_ratio: f64,
}

/// Frases que o Whisper costuma inventar em trechos de silêncio ou ruído
/// (vindas de legendas de vídeos usadas no treino).
const HALLUCINATION_PHRASES: &[&str] = &[
    "inscreva-se",
    "inscreva se",
    "assinar o canal",
    "assine o canal",
    "sininho",
    "ative o sino",
    "legendas pela comunidade",
    "legendado por",
    "legendas por",
    "obrigado por assistir",
    "amara.org",
    "até o próximo vídeo",
    "até a próxima!",
];

impl WhisperSegment {
    /// Sinais do próprio Whisper de que o trecho não é fala real: provável
    /// silêncio com baixa confiança, texto muito repetitivo ou frase típica
    /// de alucinação.
    fn is_likely_hallucination(&self) -> bool {
        let silent = self.no_speech_prob > 0.6 && self.avg_logprob < -1.0;
        let repetitive = self.compression_ratio > 2.4;
        let lower = self.text.to_lowercase();
        let known = HALLUCINATION_PHRASES.iter().any(|p| lower.contains(p));
        silent || repetitive || known
    }
}

/// Máximo de caracteres do prompt do Whisper (o limite é ~224 tokens).
const WHISPER_PROMPT_MAX_CHARS: usize = 700;

/// Texto de contexto para o Whisper acertar a grafia de nomes e termos.
pub fn whisper_prompt(title: &str, description: Option<&str>, vocabulary: &str) -> Option<String> {
    let mut parts = Vec::new();
    let terms = vocabulary_terms(vocabulary);
    if !terms.is_empty() {
        parts.push(format!("Nomes e termos: {}.", terms.join(", ")));
    }
    if !title.trim().is_empty() {
        parts.push(format!("Reunião: {}.", title.trim()));
    }
    if let Some(desc) = description.map(str::trim).filter(|d| !d.is_empty()) {
        parts.push(desc.to_string());
    }
    let prompt = parts.join(" ");
    if prompt.is_empty() {
        return None;
    }
    Some(prompt.chars().take(WHISPER_PROMPT_MAX_CHARS).collect())
}

/// Termos do vocabulário, separados por vírgula, ponto e vírgula ou linha.
pub fn vocabulary_terms(vocabulary: &str) -> Vec<String> {
    vocabulary
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

pub fn transcribe_file(
    api_key: &str,
    path: &Path,
    language: &str,
    prompt: Option<&str>,
) -> AppResult<TranscriptionResult> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    let client = http_client(TRANSCRIBE_TIMEOUT)?;
    let (status, body) = send_with_retry("Whisper", || {
        let part = reqwest::blocking::multipart::Part::bytes(bytes.clone())
            .file_name(filename.clone())
            .mime_str("audio/wav")
            .map_err(|e| AppError::Message(e.to_string()))?;

        let form = reqwest::blocking::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-1")
            .text("response_format", "verbose_json")
            .text("language", language.to_string());
        let form = match prompt {
            Some(p) => form.text("prompt", p.to_string()),
            None => form,
        };

        Ok(client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(api_key)
            .multipart(form))
    })?;

    if !status.is_success() {
        return Err(AppError::Message(format!(
            "Whisper falhou ({status}): {body}"
        )));
    }

    let parsed: WhisperVerbose = serde_json::from_str(&body)?;
    let segments: Vec<TranscriptSegment> = parsed
        .segments
        .into_iter()
        .filter(|s| !s.is_likely_hallucination())
        .map(|s| TranscriptSegment {
            start: s.start,
            end: s.end,
            text: s.text.trim().to_string(),
            speaker: None,
        })
        .filter(|s| !s.text.is_empty())
        .collect();
    // Texto montado só com os trechos mantidos (o `text` do Whisper inclui os descartados).
    let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    Ok(TranscriptionResult { text, segments })
}

pub const SUMMARY_MODEL: &str = "gpt-4o";

const SUMMARY_PROMPT: &str = r#"Você escreve atas de reunião em português do Brasil para quem não participou e precisa entender tudo o que importa sem ouvir a gravação.

A transcrição pode marcar quem fala: "Você" é a pessoa que gravou (microfone) e "Participante" é qualquer outra pessoa da chamada. Use nomes próprios quando aparecerem na conversa (apresentações, "obrigado, Ana"); não invente nomes.

Responda APENAS com JSON válido (sem markdown), neste formato:
{
  "suggested_title": "nome curto e específico da reunião, até 60 caracteres, sem data",
  "overview": "2 a 5 parágrafos separados por \n\n: objetivo da reunião, contexto, o que foi discutido e como terminou",
  "participants": [{"name": "nome ou descrição (ex.: Você, Recrutadora)", "role": "papel ou empresa, ou null"}],
  "key_points": [{"topic": "tema", "details": ["fato, argumento, número ou exemplo concreto dito na reunião"]}],
  "decisions": ["decisão tomada, com o motivo quando dito"],
  "action_items": [{"assignee": "responsável ou null", "task": "tarefa específica", "due": "prazo ou null"}],
  "open_questions": ["dúvida, risco ou ponto que ficou sem resposta"],
  "next_steps": ["próximo passo combinado"],
  "topics": ["tema em poucas palavras"],
  "notable_quotes": ["frase marcante, literal"]
}

Regras:
- Seja específico: preserve números, valores, datas, nomes de empresas, ferramentas, tecnologias e prazos citados.
- key_points: um item por tema relevante, cada um com 2 a 6 detalhes concretos. Reuniões longas costumam ter de 4 a 10 temas.
- Não generalize ("discutiram vários assuntos"); diga quais e o que foi dito.
- Seja fiel ao conteúdo. Se algo não aparecer, use lista vazia."#;

pub fn summarize_transcript(
    api_key: &str,
    transcript: &str,
    context: &MeetingContext<'_>,
) -> AppResult<(StructuredSummary, String, String)> {
    let mut user = format!("Nome atual da reunião: {}\n", context.title);
    if let Some(min) = context.duration_min {
        user.push_str(&format!("Duração: cerca de {min} minutos\n"));
    }
    let terms = vocabulary_terms(context.vocabulary);
    if !terms.is_empty() {
        user.push_str(&format!(
            "Grafia correta de nomes e termos (a transcrição pode ter errado, corrija no resumo): {}\n",
            terms.join(", ")
        ));
    }
    if let Some(desc) = context.description.filter(|d| !d.trim().is_empty()) {
        user.push_str(&format!("Descrição escrita por quem gravou: {}\n", desc.trim()));
    }
    user.push_str("\nTranscrição:\n\n");
    user.push_str(transcript);

    let payload = json!({
        "model": SUMMARY_MODEL,
        "temperature": 0.3,
        "max_tokens": 4096,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": SUMMARY_PROMPT },
            { "role": "user", "content": user }
        ]
    });

    let client = http_client(SUMMARY_TIMEOUT)?;
    let (status, body) = send_with_retry("GPT", || {
        Ok(client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&payload))
    })?;

    if !status.is_success() {
        return Err(AppError::Message(format!(
            "GPT falhou ({status}): {body}"
        )));
    }

    let completion: ChatCompletionResponse = serde_json::from_str(&body)?;
    let content = completion
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| AppError::Message("Resposta vazia do GPT".into()))?;

    match serde_json::from_str::<StructuredSummary>(&content) {
        Ok(structured) => {
            let markdown = to_markdown(&structured);
            Ok((structured, markdown, SUMMARY_MODEL.into()))
        }
        Err(_) => {
            let fallback = StructuredSummary {
                suggested_title: String::new(),
                overview: content.clone(),
                participants: vec![],
                key_points: vec![],
                decisions: vec![],
                action_items: vec![],
                open_questions: vec![],
                next_steps: vec![],
                topics: vec![],
                notable_quotes: vec![],
            };
            Ok((fallback, content, SUMMARY_MODEL.into()))
        }
    }
}

fn to_markdown(summary: &StructuredSummary) -> String {
    fn list(md: &mut String, title: &str, items: &[String], empty: &str) {
        md.push_str(&format!("\n## {title}\n\n"));
        if items.is_empty() {
            md.push_str(&format!("- ({empty})\n"));
        }
        for item in items {
            md.push_str(&format!("- {item}\n"));
        }
    }

    let mut md = String::new();
    md.push_str("## Visão geral\n\n");
    md.push_str(summary.overview.trim());
    md.push('\n');

    if !summary.participants.is_empty() {
        md.push_str("\n## Participantes\n\n");
        for p in &summary.participants {
            match p.role.as_deref().filter(|r| !r.is_empty()) {
                Some(role) => md.push_str(&format!("- {} — {role}\n", p.name)),
                None => md.push_str(&format!("- {}\n", p.name)),
            }
        }
    }

    if !summary.key_points.is_empty() {
        md.push_str("\n## Pontos principais\n");
        for kp in &summary.key_points {
            md.push_str(&format!("\n### {}\n\n", kp.topic));
            for d in &kp.details {
                md.push_str(&format!("- {d}\n"));
            }
        }
    }

    list(&mut md, "Decisões", &summary.decisions, "nenhuma");

    md.push_str("\n## Tarefas\n\n");
    if summary.action_items.is_empty() {
        md.push_str("- (nenhuma)\n");
    }
    for item in &summary.action_items {
        let who = item.assignee.as_deref().unwrap_or("sem responsável");
        let due = item
            .due
            .as_ref()
            .map(|d| format!(" (prazo: {d})"))
            .unwrap_or_default();
        md.push_str(&format!("- [{who}] {}{due}\n", item.task));
    }

    if !summary.open_questions.is_empty() {
        list(&mut md, "Perguntas em aberto", &summary.open_questions, "nenhuma");
    }
    if !summary.next_steps.is_empty() {
        list(&mut md, "Próximos passos", &summary.next_steps, "nenhum");
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, no_speech: f64, logprob: f64, compression: f64) -> WhisperSegment {
        WhisperSegment {
            start: 0.0,
            end: 1.0,
            text: text.into(),
            no_speech_prob: no_speech,
            avg_logprob: logprob,
            compression_ratio: compression,
        }
    }

    #[test]
    fn flags_likely_hallucinations() {
        assert!(!seg("Eu comecei como DevOps na GetNet.", 0.01, -0.3, 1.4).is_likely_hallucination());
        assert!(seg("Tchau!", 0.9, -1.4, 0.8).is_likely_hallucination());
        assert!(seg("Não esquece de assinar o canal e clicar no sininho.", 0.1, -0.4, 1.3)
            .is_likely_hallucination());
        assert!(seg("e aí e aí e aí e aí e aí e aí", 0.2, -0.5, 3.1).is_likely_hallucination());
        // Fala real com confiança baixa, mas sem sinal de silêncio, fica.
        assert!(!seg("O Prometheus e o Grafana.", 0.2, -1.2, 1.5).is_likely_hallucination());
    }

    #[test]
    fn builds_whisper_prompt_from_vocabulary() {
        assert_eq!(
            vocabulary_terms("Kavagy, Vivan\nGetNet; Argo CD ,\n"),
            ["Kavagy", "Vivan", "GetNet", "Argo CD"]
        );
        let prompt = whisper_prompt("Entrevista DevOps", Some("Vaga na Kavagy"), "Kavagy, Vivan").unwrap();
        assert_eq!(
            prompt,
            "Nomes e termos: Kavagy, Vivan. Reunião: Entrevista DevOps. Vaga na Kavagy"
        );
        assert_eq!(whisper_prompt("", None, ""), None);
        let long = "x, ".repeat(1000);
        assert!(whisper_prompt("", None, &long).unwrap().chars().count() <= WHISPER_PROMPT_MAX_CHARS);
    }
}
