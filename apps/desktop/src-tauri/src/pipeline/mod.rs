use crate::error::{AppError, AppResult};
use crate::openai::{self, TranscriptSegment, TranscriptionResult};
use crate::storage::{Storage, Summary};
use chrono::Utc;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::fs;
use std::path::{Path, PathBuf};

/// Whisper API limit is 25MB. Chunk ~10 minutes of audio conservatively.
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

    let transcription = match transcribe_with_chunking(&api_key, &audio_path, &language) {
        Ok(t) => t,
        Err(e) => {
            storage.set_status(meeting_id, "error", Some(&e.to_string()))?;
            return Err(e);
        }
    };

    let segments_json = serde_json::to_string(&transcription.segments)?;
    storage.save_transcript(meeting_id, &transcription.text, &segments_json)?;

    if transcription.text.trim().is_empty() {
        let empty = Summary {
            meeting_id: meeting_id.to_string(),
            overview: "A gravação não contém fala detectável.".into(),
            decisions_json: "[]".into(),
            action_items_json: "[]".into(),
            topics_json: "[]".into(),
            notable_quotes_json: "[]".into(),
            raw_markdown: "## Visão geral\n\nA gravação não contém fala detectável.\n".into(),
            model: "gpt-4o".into(),
            created_at: Utc::now().to_rfc3339(),
        };
        storage.save_summary(&empty)?;
        storage.set_status(meeting_id, "ready", None)?;
        return Ok(());
    }

    match openai::summarize_transcript(&api_key, &transcription.text) {
        Ok((structured, markdown, model)) => {
            let summary = Summary {
                meeting_id: meeting_id.to_string(),
                overview: structured.overview,
                decisions_json: serde_json::to_string(&structured.decisions)?,
                action_items_json: serde_json::to_string(&structured.action_items)?,
                topics_json: serde_json::to_string(&structured.topics)?,
                notable_quotes_json: serde_json::to_string(&structured.notable_quotes)?,
                raw_markdown: markdown,
                model,
                created_at: Utc::now().to_rfc3339(),
            };
            storage.save_summary(&summary)?;
            storage.set_status(meeting_id, "ready", None)?;
            Ok(())
        }
        Err(e) => {
            storage.set_status(meeting_id, "error", Some(&e.to_string()))?;
            Err(e)
        }
    }
}

fn transcribe_with_chunking(
    api_key: &str,
    path: &Path,
    language: &str,
) -> AppResult<TranscriptionResult> {
    let meta = fs::metadata(path)?;
    const MAX_BYTES: u64 = 24 * 1024 * 1024;

    if meta.len() <= MAX_BYTES {
        return openai::transcribe_file(api_key, path, language);
    }

    let chunks = split_wav(path, MAX_CHUNK_SECONDS)?;
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
            });
        }
        offset += chunk_duration_seconds(chunk_path)?;
        let _ = fs::remove_file(chunk_path);
    }

    if let Some(parent) = chunks.first().and_then(|p| p.parent()) {
        let _ = fs::remove_dir(parent);
    }

    Ok(TranscriptionResult {
        text: texts.join(" ").trim().to_string(),
        segments: all_segments,
    })
}

fn chunk_duration_seconds(path: &Path) -> AppResult<f64> {
    let reader = WavReader::open(path).map_err(|e| AppError::Audio(e.to_string()))?;
    let duration = reader.duration() as f64 / reader.spec().sample_rate as f64;
    Ok(duration)
}

fn split_wav(path: &Path, max_seconds: u32) -> AppResult<Vec<PathBuf>> {
    let mut reader = WavReader::open(path).map_err(|e| AppError::Audio(e.to_string()))?;
    let spec = reader.spec();
    let samples_per_chunk =
        (spec.sample_rate as u64) * (spec.channels as u64) * (max_seconds as u64);

    let chunk_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("chunks");
    fs::create_dir_all(&chunk_dir)?;

    let mut chunks = Vec::new();
    let mut chunk_index = 0usize;
    let mut current_count = 0u64;
    let mut writer: Option<WavWriter<std::io::BufWriter<std::fs::File>>> = None;

    let open_writer = |index: usize| -> AppResult<WavWriter<std::io::BufWriter<std::fs::File>>> {
        let chunk_path = chunk_dir.join(format!("chunk_{index:03}.wav"));
        WavWriter::create(
            &chunk_path,
            WavSpec {
                channels: spec.channels,
                sample_rate: spec.sample_rate,
                bits_per_sample: spec.bits_per_sample,
                sample_format: match spec.sample_format {
                    SampleFormat::Int => hound::SampleFormat::Int,
                    SampleFormat::Float => hound::SampleFormat::Float,
                },
            },
        )
        .map_err(|e| AppError::Audio(e.to_string()))
    };

    match spec.sample_format {
        SampleFormat::Int => {
            for sample in reader.samples::<i16>() {
                let sample = sample.map_err(|e| AppError::Audio(e.to_string()))?;
                if writer.is_none() {
                    let w = open_writer(chunk_index)?;
                    chunks.push(chunk_dir.join(format!("chunk_{chunk_index:03}.wav")));
                    writer = Some(w);
                    current_count = 0;
                }
                if let Some(w) = writer.as_mut() {
                    w.write_sample(sample)
                        .map_err(|e| AppError::Audio(e.to_string()))?;
                }
                current_count += 1;
                if current_count >= samples_per_chunk {
                    if let Some(w) = writer.take() {
                        w.finalize()
                            .map_err(|e| AppError::Audio(e.to_string()))?;
                    }
                    chunk_index += 1;
                }
            }
        }
        SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                let sample = sample.map_err(|e| AppError::Audio(e.to_string()))?;
                if writer.is_none() {
                    let w = open_writer(chunk_index)?;
                    chunks.push(chunk_dir.join(format!("chunk_{chunk_index:03}.wav")));
                    writer = Some(w);
                    current_count = 0;
                }
                if let Some(w) = writer.as_mut() {
                    w.write_sample(sample)
                        .map_err(|e| AppError::Audio(e.to_string()))?;
                }
                current_count += 1;
                if current_count >= samples_per_chunk {
                    if let Some(w) = writer.take() {
                        w.finalize()
                            .map_err(|e| AppError::Audio(e.to_string()))?;
                    }
                    chunk_index += 1;
                }
            }
        }
    }

    if let Some(w) = writer.take() {
        w.finalize()
            .map_err(|e| AppError::Audio(e.to_string()))?;
    }

    if chunks.is_empty() {
        return Err(AppError::Audio("Áudio vazio — nada para transcrever".into()));
    }

    Ok(chunks)
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

    if let Some(s) = summary {
        md.push_str(&s.raw_markdown);
        md.push_str("\n\n");
    }

    if let Some(t) = transcript {
        md.push_str("## Transcrição\n\n");
        md.push_str(&t.full_text);
        md.push('\n');
    }

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavSpec;

    #[test]
    fn splits_long_wav_into_multiple_chunks() {
        let dir = std::env::temp_dir().join(format!("gravador-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("long.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = WavWriter::create(&path, spec).unwrap();
            // 3 seconds of silence at 8kHz mono
            for _ in 0..(8_000 * 3) {
                writer.write_sample(0i16).unwrap();
            }
            writer.finalize().unwrap();
        }

        let chunks = split_wav(&path, 1).unwrap();
        assert!(chunks.len() >= 3, "expected >=3 chunks, got {}", chunks.len());

        for chunk in &chunks {
            let _ = fs::remove_file(chunk);
        }
        let _ = fs::remove_dir_all(dir);
    }
}
