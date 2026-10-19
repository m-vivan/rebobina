//! Recupera gravações interrompidas (crash, Cmd+Q, app reiniciado durante a
//! gravação). Nesses casos a reunião fica com status "recording" e sobram
//! `system.wav` e `mic.wav` sem o cabeçalho WAV finalizado.

use crate::audio::mix_mono_wavs;
use crate::error::AppResult;
use crate::storage::{Meeting, Storage};
use chrono::{DateTime, Duration, Utc};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const RECOVERED_MESSAGE: &str =
    "A gravação foi interrompida antes de terminar. Recuperamos o áudio; clique em Tentar de novo para transcrever.";
const LOST_MESSAGE: &str =
    "A gravação foi interrompida e não havia áudio para recuperar.";

/// Reuniões presas em "recording". Deve ser lido na abertura do app, antes de
/// qualquer gravação nova começar, para não confundir uma gravação ativa.
pub fn find_interrupted(storage: &Storage) -> AppResult<Vec<Meeting>> {
    Ok(storage
        .list_meetings(None)?
        .into_iter()
        .filter(|m| m.status == "recording")
        .collect())
}

/// Conserta e mistura o áudio das reuniões interrompidas. Pode demorar em
/// gravações longas, então roda fora da thread principal.
pub fn recover_interrupted(storage: &Storage, stuck: &[Meeting]) -> AppResult<()> {
    for meeting in stuck {
        let dir = storage.meeting_dir(&meeting.id);
        match recover_dir(&dir) {
            Ok(Some((audio_path, duration_sec))) => {
                let ended_at = DateTime::parse_from_rfc3339(&meeting.started_at)
                    .map(|d| d.with_timezone(&Utc) + Duration::seconds(duration_sec))
                    .unwrap_or_else(|_| Utc::now());
                storage.finish_recording(&meeting.id, &audio_path, ended_at, duration_sec)?;
                storage.set_status(&meeting.id, "error", Some(RECOVERED_MESSAGE))?;
                eprintln!(
                    "rebobina: recuperou gravação interrompida {} ({duration_sec}s)",
                    meeting.id
                );
            }
            Ok(None) => {
                storage.set_status(&meeting.id, "error", Some(LOST_MESSAGE))?;
            }
            Err(e) => {
                eprintln!("rebobina: falha ao recuperar {}: {e}", meeting.id);
                storage.set_status(
                    &meeting.id,
                    "error",
                    Some(&format!("{LOST_MESSAGE} ({e})")),
                )?;
            }
        }
    }
    Ok(())
}

/// Conserta as faixas da pasta e mistura em `audio.wav`.
/// Devolve o caminho e a duração em segundos, ou `None` se não havia áudio.
fn recover_dir(dir: &Path) -> AppResult<Option<(std::path::PathBuf, i64)>> {
    let output = dir.join("audio.wav");
    let system = dir.join("system.wav");
    let mic = dir.join("mic.wav");

    let has_system = repair_wav_header(&system)?.unwrap_or(0) > 0;
    let has_mic = repair_wav_header(&mic)?.unwrap_or(0) > 0;

    if has_system || has_mic {
        mix_mono_wavs(&system, &mic, &output)?;
        let _ = fs::remove_file(&system);
        let _ = fs::remove_file(&mic);
    } else if !output.exists() {
        return Ok(None);
    }

    // Se o app caiu depois da mixagem, o audio.wav já está pronto.
    let reader = hound::WavReader::open(&output)
        .map_err(|e| crate::error::AppError::Audio(e.to_string()))?;
    let spec = reader.spec();
    let frames = reader.duration() as i64;
    if frames == 0 {
        return Ok(None);
    }
    let duration = (frames / spec.sample_rate.max(1) as i64).max(1);
    Ok(Some((output, duration)))
}

/// Reescreve os tamanhos do cabeçalho de um WAV que não foi finalizado (o
/// hound só grava os tamanhos reais no `finalize`). Devolve quantos bytes de
/// áudio o arquivo tem, ou `None` se ele não existe ou não é um WAV.
pub fn repair_wav_header(path: &Path) -> AppResult<Option<u64>> {
    let Ok(mut file) = OpenOptions::new().read(true).write(true).open(path) else {
        return Ok(None);
    };
    let file_len = file.metadata()?.len();
    let mut header = vec![0u8; file_len.min(512) as usize];
    file.read_exact(&mut header)?;
    if header.len() < 12 || &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Ok(None);
    }

    // Procura o chunk "data" a partir do primeiro chunk depois de "WAVE".
    let mut offset = 12usize;
    let data_offset = loop {
        if offset + 8 > header.len() {
            return Ok(None);
        }
        let id = &header[offset..offset + 4];
        let size = u32::from_le_bytes(header[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            break offset;
        }
        offset += 8 + size + (size % 2);
    };

    let data_start = data_offset as u64 + 8;
    // Amostras de 16 bits: descarta um byte solto no fim, se houver.
    let data_len = file_len.saturating_sub(data_start) & !1;
    let riff_len = data_start + data_len - 8;

    file.seek(SeekFrom::Start(4))?;
    file.write_all(&(riff_len as u32).to_le_bytes())?;
    file.seek(SeekFrom::Start(data_offset as u64 + 4))?;
    file.write_all(&(data_len as u32).to_le_bytes())?;
    file.set_len(data_start + data_len)?;
    file.flush()?;
    Ok(Some(data_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};

    fn spec() -> WavSpec {
        WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        }
    }

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rebobina-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Simula o app caindo no meio da gravação: as amostras estão no disco, mas
    /// os tamanhos do cabeçalho continuam zerados (o `finalize` nunca rodou).
    fn write_interrupted(path: &Path, samples: usize, value: i16) {
        let mut writer = WavWriter::create(path, spec()).unwrap();
        for _ in 0..samples {
            writer.write_sample(value).unwrap();
        }
        writer.finalize().unwrap();

        let mut bytes = fs::read(path).unwrap();
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        let data = bytes.windows(4).position(|w| w == b"data").unwrap();
        bytes[data + 4..data + 8].copy_from_slice(&0u32.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn repairs_header_of_unfinished_wav() {
        let dir = temp_dir();
        let path = dir.join("mic.wav");
        write_interrupted(&path, 48_000, 1000);
        assert!(hound::WavReader::open(&path).map(|r| r.duration()).unwrap_or(0) < 48_000);

        assert_eq!(repair_wav_header(&path).unwrap(), Some(96_000));
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.duration(), 48_000);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ignores_missing_or_invalid_files() {
        let dir = temp_dir();
        assert_eq!(repair_wav_header(&dir.join("nada.wav")).unwrap(), None);
        fs::write(dir.join("lixo.wav"), b"isto nao e um wav").unwrap();
        assert_eq!(repair_wav_header(&dir.join("lixo.wav")).unwrap(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recovers_both_tracks_into_audio_wav() {
        let dir = temp_dir();
        write_interrupted(&dir.join("system.wav"), 96_000, 2000);
        write_interrupted(&dir.join("mic.wav"), 96_000, 1000);

        let (path, duration) = recover_dir(&dir).unwrap().unwrap();
        assert_eq!(path, dir.join("audio.wav"));
        assert_eq!(duration, 2);
        assert!(!dir.join("system.wav").exists());
        assert!(!dir.join("mic.wav").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reports_nothing_when_tracks_are_empty() {
        let dir = temp_dir();
        write_interrupted(&dir.join("system.wav"), 0, 0);
        assert!(recover_dir(&dir).unwrap().is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
