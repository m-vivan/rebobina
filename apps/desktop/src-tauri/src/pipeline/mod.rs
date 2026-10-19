use crate::error::{AppError, AppResult};
use crate::openai::{self, MeetingContext, TranscriptSegment, TranscriptionResult};
use crate::speakers::{self, SpeakerLevels};
use crate::storage::{Storage, Summary};
use chrono::Utc;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::fs;
use std::path::{Path, PathBuf};

/// Taxa enviada ao Whisper: ele trabalha internamente em 16 kHz, então
/// mandar mais que isso só aumenta o upload.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Blocos de 10 minutos em 16 kHz mono 16-bit têm ~19 MB, abaixo do limite
/// de 25 MB da API do Whisper.
const MAX_CHUNK_SECONDS: u32 = 600;

pub fn process_meeting(storage: &Storage, meeting_id: &str) -> AppResult<()> {
    let meeting = storage
        .get_meeting(meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))?;

    let audio_path = meeting
        .audio_path
        .ok_or_else(|| AppError::Message("Reunião sem arquivo de áudio".into()))?;
    let audio_path = PathBuf::from(audio_path);

    let api_key = Storage::get_api_key()?
        .ok_or_else(|| AppError::Message("Configure a API key da OpenAI em Configurações".into()))?;

    let settings = storage.get_settings()?;
    let language = settings.language;

    storage.set_status(meeting_id, "processing", None)?;

    let mut transcription = match transcribe_with_chunking(&api_key, &audio_path, &language) {
        Ok(t) => t,
        Err(e) => {
            storage.set_status(meeting_id, "error", Some(&e.to_string()))?;
            return Err(e);
        }
    };

    if let Some(levels) = SpeakerLevels::load_for(&audio_path) {
        levels.label_segments(&mut transcription.segments);
    }

    let segments_json = serde_json::to_string(&transcription.segments)?;
    storage.save_transcript(meeting_id, &transcription.text, &segments_json)?;

    summarize_and_save(storage, &api_key, meeting_id)
}

/// Refaz só o resumo, a partir da transcrição já salva (sem reenviar o áudio).
pub fn regenerate_summary(storage: &Storage, meeting_id: &str) -> AppResult<()> {
    let api_key = Storage::get_api_key()?
        .ok_or_else(|| AppError::Message("Configure a API key da OpenAI em Configurações".into()))?;
    storage.set_status(meeting_id, "processing", None)?;
    summarize_and_save(storage, &api_key, meeting_id)
}

fn summarize_and_save(storage: &Storage, api_key: &str, meeting_id: &str) -> AppResult<()> {
    let meeting = storage
        .get_meeting(meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))?;
    let transcript = storage
        .get_transcript(meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião ainda sem transcrição".into()))?;

    if transcript.full_text.trim().is_empty() {
        let empty = Summary {
            meeting_id: meeting_id.to_string(),
            overview: "A gravação não contém fala detectável.".into(),
            decisions_json: "[]".into(),
            action_items_json: "[]".into(),
            topics_json: "[]".into(),
            notable_quotes_json: "[]".into(),
            raw_markdown: "## Visão geral\n\nA gravação não contém fala detectável.\n".into(),
            model: openai::SUMMARY_MODEL.into(),
            created_at: Utc::now().to_rfc3339(),
            details_json: None,
        };
        storage.save_summary(&empty)?;
        storage.set_status(meeting_id, "ready", None)?;
        return Ok(());
    }

    let segments: Vec<TranscriptSegment> =
        serde_json::from_str(&transcript.segments_json).unwrap_or_default();
    let text = transcript_for_summary(&segments, &transcript.full_text);
    let context = MeetingContext {
        title: &meeting.title,
        description: meeting.description.as_deref(),
        duration_min: meeting.duration_sec.map(|s| (s + 59) / 60),
    };

    match openai::summarize_transcript(api_key, &text, &context) {
        Ok((structured, markdown, model)) => {
            let summary = Summary {
                meeting_id: meeting_id.to_string(),
                overview: structured.overview.clone(),
                decisions_json: serde_json::to_string(&structured.decisions)?,
                action_items_json: serde_json::to_string(&structured.action_items)?,
                topics_json: serde_json::to_string(&structured.topics)?,
                notable_quotes_json: serde_json::to_string(&structured.notable_quotes)?,
                raw_markdown: markdown,
                model,
                created_at: Utc::now().to_rfc3339(),
                details_json: Some(serde_json::to_string(&structured)?),
            };
            storage.save_summary(&summary)?;

            let suggested = structured.suggested_title.trim();
            if !suggested.is_empty() && is_default_title(&meeting.title) {
                storage.set_title(meeting_id, suggested)?;
            }

            storage.set_status(meeting_id, "ready", None)?;
            Ok(())
        }
        Err(e) => {
            storage.set_status(meeting_id, "error", Some(&e.to_string()))?;
            Err(e)
        }
    }
}

/// Título gerado automaticamente ao começar sem nome ("Reunião 2026-10-19 20:04").
pub fn is_default_title(title: &str) -> bool {
    let Some(rest) = title.trim().strip_prefix("Reunião ") else {
        return false;
    };
    rest.len() == 16
        && rest
            .chars()
            .enumerate()
            .all(|(i, c)| match i {
                4 | 7 => c == '-',
                10 => c == ' ',
                13 => c == ':',
                _ => c.is_ascii_digit(),
            })
}

/// Transcrição com tempo e quem fala, quando os trechos têm essa informação.
fn transcript_for_summary(segments: &[TranscriptSegment], full_text: &str) -> String {
    if segments.iter().all(|s| s.speaker.is_none()) {
        return full_text.to_string();
    }
    segments
        .iter()
        .map(|s| {
            let who = match s.speaker.as_deref() {
                Some(speakers::SPEAKER_ME) => "Você",
                Some(_) => "Participante",
                None => "?",
            };
            let t = s.start.max(0.0) as u64;
            format!("[{:02}:{:02}] {who}: {}", t / 60, t % 60, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn transcribe_with_chunking(
    api_key: &str,
    path: &Path,
    language: &str,
) -> AppResult<TranscriptionResult> {
    let chunk_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("chunks");
    let _ = fs::remove_dir_all(&chunk_dir);

    let result = (|| {
        let chunks = prepare_chunks(path, &chunk_dir, MAX_CHUNK_SECONDS)?;
        let mut all_segments = Vec::new();
        let mut texts = Vec::new();
        let mut offset = 0.0f64;

        for chunk_path in &chunks {
            let result = openai::transcribe_file(api_key, chunk_path, language)?;
            texts.push(result.text);
            for seg in result.segments {
                all_segments.push(TranscriptSegment {
                    start: seg.start + offset,
                    end: seg.end + offset,
                    text: seg.text,
                    speaker: None,
                });
            }
            offset += chunk_duration_seconds(chunk_path)?;
        }

        Ok(TranscriptionResult {
            text: texts.join(" ").trim().to_string(),
            segments: all_segments,
        })
    })();

    let _ = fs::remove_dir_all(&chunk_dir);
    result
}

fn chunk_duration_seconds(path: &Path) -> AppResult<f64> {
    let reader = WavReader::open(path).map_err(|e| AppError::Audio(e.to_string()))?;
    let duration = reader.duration() as f64 / reader.spec().sample_rate as f64;
    Ok(duration)
}

/// Converte o WAV para mono 16-bit em no máximo 16 kHz e divide em blocos de
/// até `max_seconds`, para que cada envio caiba no limite do Whisper.
/// Lê o arquivo em streaming, sem carregar a gravação inteira na memória.
fn prepare_chunks(path: &Path, chunk_dir: &Path, max_seconds: u32) -> AppResult<Vec<PathBuf>> {
    let mut reader = WavReader::open(path).map_err(|e| AppError::Audio(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let out_rate = spec.sample_rate.min(WHISPER_SAMPLE_RATE);
    // Média simples das amostras de cada janela: funciona como filtro
    // passa-baixa grosseiro e evita aliasing na redução de taxa.
    let ratio = spec.sample_rate as f64 / out_rate as f64;

    fs::create_dir_all(chunk_dir)?;
    let mut chunker = ChunkWriter::new(chunk_dir, out_rate, max_seconds);

    let mut frame_sum = 0.0f32;
    let mut frame_len = 0usize;
    let mut window_sum = 0.0f32;
    let mut window_len = 0u32;
    let mut pos = 0.0f64;
    let mut boundary = ratio;

    let mut push_sample = |value: f32, chunker: &mut ChunkWriter| -> AppResult<()> {
        frame_sum += value;
        frame_len += 1;
        if frame_len < channels {
            return Ok(());
        }
        let mono = frame_sum / channels as f32;
        frame_sum = 0.0;
        frame_len = 0;

        window_sum += mono;
        window_len += 1;
        pos += 1.0;
        if pos >= boundary {
            chunker.write(window_sum / window_len as f32)?;
            window_sum = 0.0;
            window_len = 0;
            boundary += ratio;
        }
        Ok(())
    };

    match spec.sample_format {
        SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            for sample in reader.samples::<i32>() {
                let sample = sample.map_err(|e| AppError::Audio(e.to_string()))?;
                push_sample(sample as f32 / scale, &mut chunker)?;
            }
        }
        SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                let sample = sample.map_err(|e| AppError::Audio(e.to_string()))?;
                push_sample(sample, &mut chunker)?;
            }
        }
    }

    if window_len > 0 {
        chunker.write(window_sum / window_len as f32)?;
    }

    let chunks = chunker.finish()?;
    if chunks.is_empty() {
        return Err(AppError::Audio("Áudio vazio — nada para transcrever".into()));
    }
    Ok(chunks)
}

/// Grava amostras mono 16-bit, abrindo um novo arquivo a cada `max_seconds`.
struct ChunkWriter<'a> {
    dir: &'a Path,
    spec: WavSpec,
    samples_per_chunk: u64,
    current: Option<WavWriter<std::io::BufWriter<std::fs::File>>>,
    count: u64,
    paths: Vec<PathBuf>,
}

impl<'a> ChunkWriter<'a> {
    fn new(dir: &'a Path, sample_rate: u32, max_seconds: u32) -> Self {
        Self {
            dir,
            spec: WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
            samples_per_chunk: sample_rate as u64 * max_seconds.max(1) as u64,
            current: None,
            count: 0,
            paths: Vec::new(),
        }
    }

    fn write(&mut self, sample: f32) -> AppResult<()> {
        if self.current.is_none() {
            let path = self.dir.join(format!("chunk_{:03}.wav", self.paths.len()));
            let writer =
                WavWriter::create(&path, self.spec).map_err(|e| AppError::Audio(e.to_string()))?;
            self.paths.push(path);
            self.current = Some(writer);
            self.count = 0;
        }
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        if let Some(w) = self.current.as_mut() {
            w.write_sample(value)
                .map_err(|e| AppError::Audio(e.to_string()))?;
        }
        self.count += 1;
        if self.count >= self.samples_per_chunk {
            self.close_current()?;
        }
        Ok(())
    }

    fn close_current(&mut self) -> AppResult<()> {
        if let Some(w) = self.current.take() {
            w.finalize().map_err(|e| AppError::Audio(e.to_string()))?;
        }
        Ok(())
    }

    fn finish(mut self) -> AppResult<Vec<PathBuf>> {
        self.close_current()?;
        Ok(self.paths)
    }
}

pub fn export_markdown(storage: &Storage, meeting_id: &str) -> AppResult<String> {
    let meeting = storage
        .get_meeting(meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))?;
    let transcript = storage.get_transcript(meeting_id)?;
    let summary = storage.get_summary(meeting_id)?;

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", meeting.title));
    md.push_str(&format!("- Início: {}\n", meeting.started_at));
    if let Some(ended) = &meeting.ended_at {
        md.push_str(&format!("- Fim: {ended}\n"));
    }
    if let Some(dur) = meeting.duration_sec {
        md.push_str(&format!("- Duração: {dur}s\n"));
    }
    md.push('\n');

    if let Some(desc) = meeting.description.as_deref().filter(|d| !d.trim().is_empty()) {
        md.push_str(desc.trim());
        md.push_str("\n\n");
    }

    if let Some(s) = summary {
        md.push_str(&s.raw_markdown);
        md.push_str("\n\n");
    }

    if let Some(t) = transcript {
        md.push_str("## Transcrição\n\n");
        let segments: Vec<TranscriptSegment> =
            serde_json::from_str(&t.segments_json).unwrap_or_default();
        // Linha em branco entre falas para o Markdown não juntar tudo num parágrafo.
        md.push_str(&transcript_for_summary(&segments, &t.full_text).replace('\n', "\n\n"));
        md.push('\n');
    }

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavSpec;

    fn write_wav(path: &Path, spec: WavSpec, frames: usize) {
        let mut writer = WavWriter::create(path, spec).unwrap();
        for i in 0..frames * spec.channels as usize {
            match spec.sample_format {
                hound::SampleFormat::Int => writer.write_sample((i % 200) as i16).unwrap(),
                hound::SampleFormat::Float => writer.write_sample(0.1f32).unwrap(),
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn splits_long_wav_into_multiple_chunks() {
        let dir = std::env::temp_dir().join(format!("rebobina-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("long.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        // 3 segundos em 8 kHz: abaixo de 16 kHz, mantém a taxa original
        write_wav(&path, spec, 8_000 * 3);

        let chunks = prepare_chunks(&path, &dir.join("chunks"), 1).unwrap();
        assert_eq!(chunks.len(), 3);
        for chunk in &chunks {
            let reader = WavReader::open(chunk).unwrap();
            assert_eq!(reader.spec().sample_rate, 8_000);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn downsamples_48k_stereo_to_16k_mono() {
        let dir = std::env::temp_dir().join(format!("rebobina-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hq.wav");
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        write_wav(&path, spec, 48_000 * 3);

        let chunks = prepare_chunks(&path, &dir.join("chunks"), 1).unwrap();
        assert_eq!(chunks.len(), 3);
        let mut total = 0.0;
        for chunk in &chunks {
            let reader = WavReader::open(chunk).unwrap();
            let spec = reader.spec();
            assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16_000, 16));
            total += chunk_duration_seconds(chunk).unwrap();
        }
        assert!((total - 3.0).abs() < 0.01, "duração total {total}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn detects_only_the_automatic_title() {
        assert!(is_default_title("Reunião 2026-10-19 20:04"));
        assert!(!is_default_title("Reunião de planejamento"));
        assert!(!is_default_title("Daily 2026-10-19 20:04"));
    }

    #[test]
    fn summary_text_marks_who_speaks() {
        let segments = vec![
            TranscriptSegment { start: 3.0, end: 5.0, text: "Oi, tudo bem?".into(), speaker: Some("me".into()) },
            TranscriptSegment { start: 65.0, end: 70.0, text: "Tudo ótimo.".into(), speaker: Some("others".into()) },
        ];
        let text = transcript_for_summary(&segments, "ignorado");
        assert_eq!(text, "[00:03] Você: Oi, tudo bem?\n[01:05] Participante: Tudo ótimo.");
    }

    #[test]
    fn ten_minute_chunk_fits_whisper_limit() {
        let bytes = WHISPER_SAMPLE_RATE as u64 * MAX_CHUNK_SECONDS as u64 * 2 + 44;
        assert!(bytes < 25 * 1024 * 1024, "{bytes} bytes");
    }
}
