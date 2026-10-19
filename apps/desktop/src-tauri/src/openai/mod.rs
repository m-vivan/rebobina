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
    pub overview: String,
    pub decisions: Vec<String>,
    pub action_items: Vec<ActionItem>,
    pub topics: Vec<String>,
    pub notable_quotes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WhisperVerbose {
    text: String,
    #[serde(default)]
    segments: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    start: f64,
    end: f64,
    text: String,
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

pub fn transcribe_file(api_key: &str, path: &Path, language: &str) -> AppResult<TranscriptionResult> {
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
    Ok(TranscriptionResult {
        text: parsed.text.trim().to_string(),
        segments: parsed
            .segments
            .into_iter()
            .map(|s| TranscriptSegment {
                start: s.start,
                end: s.end,
                text: s.text.trim().to_string(),
            })
            .collect(),
    })
}

pub fn summarize_transcript(api_key: &str, transcript: &str) -> AppResult<(StructuredSummary, String, String)> {
    let system = r#"Você é um assistente que resume reuniões em português do Brasil.
Responda APENAS com JSON válido (sem markdown) no formato:
{
  "overview": "string",
  "decisions": ["string"],
  "action_items": [{"assignee": "string|null", "task": "string", "due": "string|null"}],
  "topics": ["string"],
  "notable_quotes": ["string"]
}
Seja fiel ao conteúdo. Se algo não aparecer, use lista vazia."#;

    let user = format!(
        "Resuma a seguinte transcrição de reunião:\n\n{}",
        transcript
    );

    let payload = json!({
        "model": "gpt-4o",
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system },
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
            Ok((structured, markdown, "gpt-4o".into()))
        }
        Err(_) => {
            let fallback = StructuredSummary {
                overview: content.clone(),
                decisions: vec![],
                action_items: vec![],
                topics: vec![],
                notable_quotes: vec![],
            };
            Ok((fallback, content, "gpt-4o".into()))
        }
    }
}

fn to_markdown(summary: &StructuredSummary) -> String {
    let mut md = String::new();
    md.push_str("## Visão geral\n\n");
    md.push_str(&summary.overview);
    md.push_str("\n\n## Decisões\n\n");
    if summary.decisions.is_empty() {
        md.push_str("- (nenhuma)\n");
    } else {
        for d in &summary.decisions {
            md.push_str(&format!("- {d}\n"));
        }
    }
    md.push_str("\n## Action items\n\n");
    if summary.action_items.is_empty() {
        md.push_str("- (nenhum)\n");
    } else {
        for item in &summary.action_items {
            let who = item.assignee.as_deref().unwrap_or("não atribuído");
            let due = item
                .due
                .as_ref()
                .map(|d| format!(" (prazo: {d})"))
                .unwrap_or_default();
            md.push_str(&format!("- [{}] {}{}\n", who, item.task, due));
        }
    }
    md.push_str("\n## Tópicos\n\n");
    if summary.topics.is_empty() {
        md.push_str("- (nenhum)\n");
    } else {
        for t in &summary.topics {
            md.push_str(&format!("- {t}\n"));
        }
    }
    md
}
