//! Identifica quem fala em cada trecho comparando o volume das duas faixas
//! gravadas: microfone (você) e áudio do sistema (as outras pessoas da chamada).

use crate::error::AppResult;
use crate::openai::TranscriptSegment;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "speakers.json";
const WINDOW_MS: u32 = 100;

pub const SPEAKER_ME: &str = "me";
pub const SPEAKER_OTHERS: &str = "others";

/// Nível médio (0–255) abaixo do qual um trecho é considerado silêncio.
const SILENCE_LEVEL: f64 = 3.0;

/// Volume (RMS, 0–255) de cada faixa em janelas de `window_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerLevels {
    pub window_ms: u32,
    pub mic: Vec<u8>,
    pub system: Vec<u8>,
}

impl SpeakerLevels {
    pub fn from_tracks(mic: &[f32], mic_rate: u32, system: &[f32], system_rate: u32) -> Self {
        Self {
            window_ms: WINDOW_MS,
            mic: rms_windows(mic, mic_rate),
            system: rms_windows(system, system_rate),
        }
    }

    pub fn path_for(audio_path: &Path) -> PathBuf {
        audio_path.with_file_name(FILE_NAME)
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        std::fs::write(path, serde_json::to_vec(self)?)?;
        Ok(())
    }

    /// Lê os níveis gravados junto do áudio, se existirem (reuniões antigas não têm).
    pub fn load_for(audio_path: &Path) -> Option<Self> {
        let bytes = std::fs::read(Self::path_for(audio_path)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Quem fala entre `start` e `end` (segundos), pela faixa mais alta.
    pub fn speaker_between(&self, start: f64, end: f64) -> Option<&'static str> {
        let per_sec = 1000.0 / self.window_ms.max(1) as f64;
        let from = (start.max(0.0) * per_sec).floor() as usize;
        let to = ((end.max(start) * per_sec).ceil() as usize).max(from + 1);
        let energy = |track: &[u8]| -> f64 {
            track
                .iter()
                .skip(from)
                .take(to - from)
                .map(|&v| (v as f64).powi(2))
                .sum()
        };
        let mic = energy(&self.mic);
        let system = energy(&self.system);
        if mic == 0.0 && system == 0.0 {
            None
        } else if mic > system {
            Some(SPEAKER_ME)
        } else {
            Some(SPEAKER_OTHERS)
        }
    }

    /// As duas faixas praticamente mudas entre `start` e `end` (média do nível
    /// mais alto abaixo de `SILENCE_LEVEL`). Fala real fica bem acima disso.
    pub fn is_silent_between(&self, start: f64, end: f64) -> bool {
        let per_sec = 1000.0 / self.window_ms.max(1) as f64;
        let from = (start.max(0.0) * per_sec).floor() as usize;
        let to = ((end.max(start) * per_sec).ceil() as usize).max(from + 1);
        let n = to - from;
        let loudest: u32 = (from..to)
            .map(|i| {
                let m = self.mic.get(i).copied().unwrap_or(0);
                let s = self.system.get(i).copied().unwrap_or(0);
                m.max(s) as u32
            })
            .sum();
        (loudest as f64 / n as f64) < SILENCE_LEVEL
    }

    pub fn label_segments(&self, segments: &mut [TranscriptSegment]) {
        for seg in segments {
            seg.speaker = self.speaker_between(seg.start, seg.end).map(str::to_string);
        }
    }
}

fn rms_windows(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let window = ((sample_rate as u64 * WINDOW_MS as u64) / 1000).max(1) as usize;
    samples
        .chunks(window)
        .map(|chunk| {
            let mean_sq = chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32;
            (mean_sq.sqrt().clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: String::new(),
            speaker: None,
        }
    }

    #[test]
    fn labels_who_is_louder_in_each_segment() {
        let rate = 1_000;
        // 0–1 s: só o microfone; 1–2 s: só o sistema; 2–3 s: silêncio.
        let mut mic = vec![0.5f32; rate];
        mic.extend(vec![0.02f32; rate]);
        mic.extend(vec![0.0f32; rate]);
        let mut system = vec![0.0f32; rate];
        system.extend(vec![0.6f32; rate]);
        system.extend(vec![0.0f32; rate]);

        let levels = SpeakerLevels::from_tracks(&mic, rate as u32, &system, rate as u32);
        assert_eq!(levels.mic.len(), 30);

        let mut segments = vec![seg(0.0, 1.0), seg(1.0, 2.0), seg(2.1, 2.9)];
        levels.label_segments(&mut segments);
        assert_eq!(segments[0].speaker.as_deref(), Some(SPEAKER_ME));
        assert_eq!(segments[1].speaker.as_deref(), Some(SPEAKER_OTHERS));
        assert_eq!(segments[2].speaker, None);
    }

    #[test]
    fn saves_and_loads_next_to_audio() {
        let dir = std::env::temp_dir().join(format!("rebobina-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("audio.wav");
        let levels = SpeakerLevels::from_tracks(&[0.1; 800], 8_000, &[0.2; 800], 8_000);
        levels.save(&SpeakerLevels::path_for(&audio)).unwrap();
        assert_eq!(SpeakerLevels::load_for(&audio), Some(levels));
        let _ = std::fs::remove_dir_all(dir);
    }
}
