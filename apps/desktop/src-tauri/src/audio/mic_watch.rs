//! Percebe quando o microfone gravado está mudo enquanto a chamada tem fala:
//! sinal de que a pessoa está falando em outro microfone (ex.: no AirPods,
//! com o app gravando o do MacBook).

use std::time::{Duration, Instant};

/// Pico abaixo do qual o microfone é considerado mudo (ruído de ambiente).
pub const MIC_SILENT_PEAK: f32 = 0.03;
/// Pico acima do qual há fala na chamada.
pub const CALL_SPEECH_PEAK: f32 = 0.05;
/// Tempo de microfone mudo antes de procurar a voz em outro microfone.
const MIC_SILENT_FOR: Duration = Duration::from_secs(20);
/// Fala na chamada dentro desse período, para não agir numa reunião parada.
const CALL_SPEECH_FOR: Duration = Duration::from_secs(5);
/// Intervalo mínimo entre duas buscas.
const PROBE_COOLDOWN: Duration = Duration::from_secs(90);

#[derive(Default)]
pub struct SilenceWatch {
    mic_silent_for: Duration,
    call_speech_for: Duration,
    last_probe: Option<Instant>,
}

impl SilenceWatch {
    /// Registra os picos dos últimos `dt` e diz se é hora de procurar a voz
    /// em outro microfone.
    pub fn update(
        &mut self,
        mic_peak: f32,
        call_peak: f32,
        dt: Duration,
        paused: bool,
        now: Instant,
    ) -> bool {
        if paused || mic_peak >= MIC_SILENT_PEAK {
            self.mic_silent_for = Duration::ZERO;
            self.call_speech_for = Duration::ZERO;
            return false;
        }
        self.mic_silent_for += dt;
        if call_peak >= CALL_SPEECH_PEAK {
            self.call_speech_for += dt;
        }
        let cooled = self
            .last_probe
            .is_none_or(|t| now.saturating_duration_since(t) >= PROBE_COOLDOWN);
        if self.mic_silent_for >= MIC_SILENT_FOR && self.call_speech_for >= CALL_SPEECH_FOR && cooled
        {
            self.last_probe = Some(now);
            self.mic_silent_for = Duration::ZERO;
            self.call_speech_for = Duration::ZERO;
            return true;
        }
        false
    }

    /// Depois de uma troca de microfone, recomeça a contagem.
    pub fn reset(&mut self) {
        self.mic_silent_for = Duration::ZERO;
        self.call_speech_for = Duration::ZERO;
    }
}

/// Entre os níveis medidos, o microfone com voz clara (`None` se nenhum).
pub fn pick_voice(levels: &[(String, f32)]) -> Option<String> {
    /// RMS mínimo de fala perto do microfone.
    const VOICE_RMS: f32 = 0.02;
    levels
        .iter()
        .filter(|(_, rms)| *rms >= VOICE_RMS)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(100);

    fn run(watch: &mut SilenceWatch, secs: u32, mic: f32, call: f32, start: &mut Instant) -> bool {
        let mut fired = false;
        for _ in 0..secs * 10 {
            fired |= watch.update(mic, call, TICK, false, *start);
            *start += TICK;
        }
        fired
    }

    #[test]
    fn fires_when_mic_is_mute_while_call_has_speech() {
        let mut w = SilenceWatch::default();
        let mut t = Instant::now();
        assert!(!run(&mut w, 15, 0.005, 0.2, &mut t));
        assert!(run(&mut w, 10, 0.005, 0.2, &mut t));
        // Logo depois, respeita o intervalo entre buscas.
        assert!(!run(&mut w, 30, 0.005, 0.2, &mut t));
        assert!(run(&mut w, 90, 0.005, 0.2, &mut t));
    }

    #[test]
    fn stays_quiet_when_nobody_talks_or_mic_works() {
        let mut w = SilenceWatch::default();
        let mut t = Instant::now();
        // Reunião parada: microfone e chamada em silêncio.
        assert!(!run(&mut w, 60, 0.005, 0.0, &mut t));
        // Microfone com voz: tudo certo.
        assert!(!run(&mut w, 60, 0.2, 0.2, &mut t));
        // Pausada: não conta.
        for _ in 0..600 {
            assert!(!w.update(0.0, 0.3, TICK, true, t));
            t += TICK;
        }
    }

    #[test]
    fn picks_microphone_with_clear_voice() {
        let levels = vec![
            ("Microfone (MacBook Pro)".to_string(), 0.004),
            ("AirPods Pro de Vivan".to_string(), 0.09),
            ("Brio 500".to_string(), 0.01),
        ];
        assert_eq!(pick_voice(&levels).as_deref(), Some("AirPods Pro de Vivan"));
        assert_eq!(pick_voice(&[("Brio 500".to_string(), 0.005)]), None);
    }
}
