mod mix;
pub use mix::mix_mono_wavs;
mod permissions;

pub use permissions::{
    ensure_screen_permission, init_screen_permission_baseline, open_screen_privacy_settings,
    reset_screen_permission_tcc, screen_permission_status, ScreenPermissionStatus,
};

use crate::error::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{WavSpec, WavWriter};
use parking_lot::Mutex;
use screencapturekit::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub likely_loopback: bool,
}

#[allow(dead_code)]
struct SendStream(Stream);
unsafe impl Send for SendStream {}

struct CaptureShared {
    system_writer: Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>,
    mic_writer: Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>,
    /// When true, mic samples come from ScreenCaptureKit (not cpal).
    sck_mic: AtomicBool,
    /// Pico de cada faixa desde a última leitura (zerado por `take_levels`).
    mic_peak: AtomicU64,
    system_peak: AtomicU64,
    /// Amostras já gravadas em cada faixa, para manter as duas alinhadas
    /// quando o microfone é trocado no meio da gravação.
    mic_samples: AtomicU64,
    system_samples: AtomicU64,
    stop: AtomicBool,
}

struct CaptureHandler {
    shared: Arc<CaptureShared>,
}

impl SCStreamOutputTrait for CaptureHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
        if self.shared.stop.load(Ordering::Relaxed) {
            return;
        }
        match output_type {
            SCStreamOutputType::Screen => {
                // Descartamos frames de vídeo (só precisamos do áudio do sistema).
            }
            SCStreamOutputType::Audio => {
                write_pcm_mono(
                    &self.shared.system_writer,
                    &sample,
                    &self.shared.system_peak,
                    &self.shared.system_samples,
                );
            }
            SCStreamOutputType::Microphone => {
                if self.shared.sck_mic.load(Ordering::Relaxed) {
                    write_pcm_mono(
                        &self.shared.mic_writer,
                        &sample,
                        &self.shared.mic_peak,
                        &self.shared.mic_samples,
                    );
                }
            }
        }
    }
}

struct ActiveRecording {
    _stream: SCStream,
    /// O SCK também está capturando o microfone (macOS 15+), podendo servir de reserva.
    sck_mic_available: bool,
    mic_stream: Option<SendStream>,
    shared: Arc<CaptureShared>,
    started_at: Instant,
    meeting_id: String,
    output_path: PathBuf,
    system_path: PathBuf,
    mic_path: PathBuf,
}

pub struct Recorder {
    active: Mutex<Option<ActiveRecording>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }

    pub fn list_input_devices() -> AppResult<Vec<AudioDeviceInfo>> {
        let host = cpal::default_host();
        let default_name = host.default_input_device().and_then(|d| d.name().ok());
        let mut devices = Vec::new();
        let input_devices = host
            .input_devices()
            .map_err(|e| AppError::Audio(e.to_string()))?;

        for device in input_devices {
            let name = device
                .name()
                .map_err(|e| AppError::Audio(e.to_string()))?;
            let channels = device
                .default_input_config()
                .map(|c| c.channels())
                .unwrap_or(1);
            let is_default = default_name.as_ref() == Some(&name);
            let likely_loopback = {
                let lower = name.to_lowercase();
                lower.contains("blackhole")
                    || lower.contains("aggregate")
                    || lower.contains("agregado")
                    || channels >= 3
            };
            devices.push(AudioDeviceInfo {
                name,
                is_default,
                channels,
                likely_loopback,
            });
        }

        if devices.is_empty() {
            return Err(AppError::Audio(
                "Nenhum microfone encontrado".into(),
            ));
        }
        Ok(devices)
    }

    pub fn is_recording(&self) -> bool {
        self.active.lock().is_some()
    }

    pub fn current_meeting_id(&self) -> Option<String> {
        self.active.lock().as_ref().map(|a| a.meeting_id.clone())
    }

    /// Pico do microfone e do áudio da chamada desde a última chamada, e zera
    /// os contadores para a próxima janela.
    pub fn take_levels(&self) -> (f32, f32) {
        let guard = self.active.lock();
        let Some(active) = guard.as_ref() else {
            return (0.0, 0.0);
        };
        let take = |peak: &AtomicU64| f32::from_bits(peak.swap(0, Ordering::Relaxed) as u32);
        (take(&active.shared.mic_peak), take(&active.shared.system_peak))
    }

    pub fn start(
        &self,
        meeting_id: &str,
        device_name: Option<&str>,
        output_path: &Path,
    ) -> AppResult<()> {
        let mut guard = self.active.lock();
        if guard.is_some() {
            return Err(AppError::Audio("Já existe uma gravação em andamento".into()));
        }

        ensure_screen_permission()?;

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let system_path = output_path.with_file_name("system.wav");
        let mic_path = output_path.with_file_name("mic.wav");

        let mono_spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let system_writer = WavWriter::create(&system_path, mono_spec)
            .map_err(|e| AppError::Audio(format!("Falha ao criar system.wav: {e}")))?;
        let mic_writer = WavWriter::create(&mic_path, mono_spec)
            .map_err(|e| AppError::Audio(format!("Falha ao criar mic.wav: {e}")))?;

        let shared = Arc::new(CaptureShared {
            system_writer: Mutex::new(Some(system_writer)),
            mic_writer: Mutex::new(Some(mic_writer)),
            sck_mic: AtomicBool::new(false),
            mic_peak: AtomicU64::new(0),
            system_peak: AtomicU64::new(0),
            mic_samples: AtomicU64::new(0),
            system_samples: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        });

        let content = SCShareableContent::get().map_err(|e| {
            AppError::Audio(format!(
                "Falha ao acessar ScreenCaptureKit ({e}). Verifique a permissão de Gravação de tela em Ajustes do Sistema."
            ))
        })?;
        let display = content.displays().into_iter().next().ok_or_else(|| {
            AppError::Audio("Nenhum display encontrado para captura de áudio".into())
        })?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| AppError::Audio(format!("Filtro SCK: {e}")))?;

        let frame_interval = CMTime::new(1, 1); // 1 fps — só precisamos do áudio
        let mut config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&frame_interval)
            .with_captures_audio(true)
            .with_sample_rate(SAMPLE_RATE as i32)
            .with_channel_count(2);

        let use_sck_mic = match config.clone().with_captures_microphone(true) {
            Ok(cfg) => {
                config = cfg;
                shared.sck_mic.store(true, Ordering::Relaxed);
                true
            }
            Err(_) => false, // macOS < 15: microfone via cpal
        };
        // O microfone do SCK é sempre o padrão do sistema (trocar o dispositivo dele
        // com a captura rodando entrega só silêncio). Para outro microfone, usa cpal.
        let wants_other_mic = device_name.is_some_and(|name| !is_default_input(name));
        if wants_other_mic {
            shared.sck_mic.store(false, Ordering::Relaxed);
        }

        let mut stream =
            SCStream::new(&filter, &config).map_err(|e| AppError::Audio(format!("SCStream: {e}")))?;

        let handler = CaptureHandler {
            shared: Arc::clone(&shared),
        };
        stream
            .add_output_handler(
                CaptureHandler {
                    shared: Arc::clone(&shared),
                },
                SCStreamOutputType::Screen,
            )
            .map_err(|e| AppError::Audio(format!("handler screen: {e}")))?;
        stream
            .add_output_handler(
                CaptureHandler {
                    shared: Arc::clone(&shared),
                },
                SCStreamOutputType::Audio,
            )
            .map_err(|e| AppError::Audio(format!("handler audio: {e}")))?;

        if use_sck_mic {
            stream
                .add_output_handler(handler, SCStreamOutputType::Microphone)
                .map_err(|e| AppError::Audio(format!("handler mic: {e}")))?;
        }

        stream.start_capture().map_err(|e| {
            let raw = e.to_string();
            // macOS localiza "user declined/refused TCCs…" — toggle ligado ≠ SCK liberado (ad-hoc / sem relaunch).
            if raw.to_lowercase().contains("tcc")
                || raw.contains("recusou")
                || raw.to_lowercase().contains("denied")
                || raw.to_lowercase().contains("declined")
            {
                AppError::Audio(permissions::tcc_capture_denied_message(&raw))
            } else {
                AppError::Audio(format!(
                    "Não foi possível iniciar a captura ({raw}). Em Ajustes → Privacidade → Gravação de tela, confira o Rebobina e reinicie o app (Cmd+Q)."
                ))
            }
        })?;

        let mic_stream = if use_sck_mic && !wants_other_mic {
            None
        } else {
            Some(start_cpal_mic(
                device_name,
                Arc::clone(&shared),
            )?)
        };

        eprintln!(
            "rebobina: ScreenCaptureKit ativo (system audio + mic via {})",
            if shared.sck_mic.load(Ordering::Relaxed) { "SCK" } else { "cpal" }
        );

        *guard = Some(ActiveRecording {
            _stream: stream,
            sck_mic_available: use_sck_mic,
            mic_stream,
            shared,
            started_at: Instant::now(),
            meeting_id: meeting_id.to_string(),
            output_path: output_path.to_path_buf(),
            system_path,
            mic_path,
        });

        Ok(())
    }

    /// Troca o microfone sem interromper a gravação. `None` volta ao padrão do sistema.
    pub fn switch_microphone(&self, device_name: Option<&str>) -> AppResult<()> {
        let mut guard = self.active.lock();
        let active = guard
            .as_mut()
            .ok_or_else(|| AppError::Audio("Nenhuma gravação ativa".into()))?;
        let shared = Arc::clone(&active.shared);

        // Fecha o microfone atual (cpal) e ignora o do SCK enquanto o novo abre.
        active.mic_stream = None;
        shared.sck_mic.store(false, Ordering::Relaxed);

        let use_sck = active.sck_mic_available
            && device_name.map_or(true, is_default_input);
        let result = if use_sck {
            shared.sck_mic.store(true, Ordering::Relaxed);
            Ok(())
        } else {
            match start_cpal_mic(device_name, Arc::clone(&shared)) {
                Ok(stream) => {
                    active.mic_stream = Some(stream);
                    Ok(())
                }
                Err(e) => {
                    // Melhor continuar com o microfone padrão do que ficar sem voz.
                    if active.sck_mic_available {
                        shared.sck_mic.store(true, Ordering::Relaxed);
                    }
                    Err(e)
                }
            }
        };

        pad_mic_to_system(&shared);
        if result.is_ok() {
            eprintln!(
                "rebobina: microfone trocado para {} (via {})",
                device_name.unwrap_or("o padrão do sistema"),
                if use_sck { "SCK" } else { "cpal" }
            );
        }
        result
    }

    pub fn stop(&self) -> AppResult<(String, PathBuf, i64)> {
        let mut guard = self.active.lock();
        let active = guard
            .take()
            .ok_or_else(|| AppError::Audio("Nenhuma gravação ativa".into()))?;

        active.shared.stop.store(true, Ordering::Relaxed);
        let duration_sec = active.started_at.elapsed().as_secs() as i64;
        let meeting_id = active.meeting_id.clone();
        let output_path = active.output_path.clone();
        let system_path = active.system_path.clone();
        let mic_path = active.mic_path.clone();

        // Drop SCStream / cpal (para a captura)
        drop(active._stream);
        drop(active.mic_stream);

        if let Some(w) = active.shared.system_writer.lock().take() {
            w.finalize()
                .map_err(|e| AppError::Audio(format!("finalize system: {e}")))?;
        }
        if let Some(w) = active.shared.mic_writer.lock().take() {
            w.finalize()
                .map_err(|e| AppError::Audio(format!("finalize mic: {e}")))?;
        }

        mix::mix_mono_wavs(&system_path, &mic_path, &output_path)?;

        let _ = std::fs::remove_file(&system_path);
        let _ = std::fs::remove_file(&mic_path);

        Ok((meeting_id, output_path, duration_sec.max(1)))
    }
}

fn write_pcm_mono(
    writer: &Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>,
    sample: &CMSampleBuffer,
    peak: &AtomicU64,
    written: &AtomicU64,
) {
    let Ok(list) = sample.audio_buffer_list() else {
        return;
    };

    // SCK tipicamente entrega float32 planar (1 buffer por canal).
    // Tratar cada buffer como stereo interleaved fazia a gravação soar ~2× acelerada.
    let mut channel_bufs: Vec<Vec<f32>> = Vec::new();
    for buffer in &list {
        let data = buffer.data();
        if data.is_empty() {
            continue;
        }
        let floats: Vec<f32> = data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        if floats.is_empty() {
            continue;
        }

        let nch = buffer.number_channels.max(1) as usize;
        if nch == 1 {
            channel_bufs.push(floats);
        } else {
            // Um único buffer interleaved multi-canal
            let mut mono = Vec::with_capacity(floats.len() / nch);
            for frame in floats.chunks(nch) {
                if frame.len() < nch {
                    break;
                }
                let sum: f32 = frame.iter().sum();
                mono.push(sum / nch as f32);
            }
            channel_bufs.push(mono);
        }
    }

    let mut mono_samples = mix_planar_channels(&channel_bufs);
    if mono_samples.is_empty() {
        return;
    }

    // Se a taxa real diferir do WAV (48 kHz), reamostra para manter duração correta.
    if let Some(fmt) = sample.format_description() {
        if let Some(rate) = fmt.audio_sample_rate() {
            let in_rate = rate.round() as u32;
            if in_rate > 0 && in_rate != SAMPLE_RATE {
                mono_samples = resample_linear(&mono_samples, in_rate, SAMPLE_RATE);
            }
        }
    }

    let mut max_amp = 0f32;
    for &s in &mono_samples {
        max_amp = max_amp.max(s.abs());
    }
    if max_amp > 0.0 {
        let prev = f32::from_bits(peak.load(Ordering::Relaxed) as u32);
        if max_amp > prev {
            peak.store(max_amp.to_bits() as u64, Ordering::Relaxed);
        }
    }

    if let Some(w) = writer.lock().as_mut() {
        for &s in &mono_samples {
            let i = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            let _ = w.write_sample(i);
        }
        written.fetch_add(mono_samples.len() as u64, Ordering::Relaxed);
    }
}

/// Mistura canais planares (mesmo índice = mesmo instante) em mono.
fn mix_planar_channels(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    if channels.len() == 1 {
        return channels[0].clone();
    }
    let len = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    let n = channels.len() as f32;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let sum: f32 = channels.iter().map(|c| c[i]).sum();
        out.push(sum / n);
    }
    out
}

fn resample_linear(mono: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = out_rate as f64 / in_rate as f64;
    let out_len = ((mono.len() as f64) * ratio).round() as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(mono.len() - 1);
        let t = (src - i0 as f64) as f32;
        out.push(mono[i0] * (1.0 - t) + mono[i1] * t);
    }
    out
}

fn start_cpal_mic(
    device_name: Option<&str>,
    shared: Arc<CaptureShared>,
) -> AppResult<SendStream> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => {
            let mut found = None;
            for d in host
                .input_devices()
                .map_err(|e| AppError::Audio(e.to_string()))?
            {
                if d.name().map_err(|e| AppError::Audio(e.to_string()))? == name {
                    found = Some(d);
                    break;
                }
            }
            found.ok_or_else(|| {
                AppError::Audio(format!("Microfone não encontrado: {name}"))
            })?
        }
        None => host
            .default_input_device()
            .ok_or_else(|| AppError::Audio("Sem microfone padrão".into()))?,
    };

    let config = device
        .default_input_config()
        .map_err(|e| AppError::Audio(e.to_string()))?;
    let channels = config.channels().max(1) as usize;
    let in_rate = config.sample_rate().0;

    let shared_cb = Arc::clone(&shared);
    let err_fn = |err| eprintln!("erro mic cpal: {err}");

    let stream = match config.sample_format() {
        SampleFormat::F32 => {
            let stream_config: cpal::StreamConfig = config.clone().into();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        if shared_cb.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let mono = downmix_and_resample_f32(data, channels, in_rate, SAMPLE_RATE);
                        write_i16_mono(
                            &shared_cb.mic_writer,
                            &mono,
                            &shared_cb.mic_peak,
                            &shared_cb.mic_samples,
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(e.to_string()))?
        }
        SampleFormat::I16 => {
            let stream_config: cpal::StreamConfig = config.clone().into();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        if shared_cb.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let f: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        let mono = downmix_and_resample_f32(&f, channels, in_rate, SAMPLE_RATE);
                        write_i16_mono(
                            &shared_cb.mic_writer,
                            &mono,
                            &shared_cb.mic_peak,
                            &shared_cb.mic_samples,
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(e.to_string()))?
        }
        other => {
            return Err(AppError::Audio(format!(
                "Formato de microfone não suportado: {other:?}"
            )));
        }
    };

    stream
        .play()
        .map_err(|e| AppError::Audio(e.to_string()))?;
    Ok(SendStream(stream))
}

fn write_i16_mono(
    writer: &Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>,
    samples: &[f32],
    peak: &AtomicU64,
    written: &AtomicU64,
) {
    let mut max_amp = 0f32;
    if let Some(w) = writer.lock().as_mut() {
        for &s in samples {
            max_amp = max_amp.max(s.abs());
            let i = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            let _ = w.write_sample(i);
        }
        written.fetch_add(samples.len() as u64, Ordering::Relaxed);
    }
    if max_amp > 0.0 {
        let prev = f32::from_bits(peak.load(Ordering::Relaxed) as u32);
        if max_amp > prev {
            peak.store(max_amp.to_bits() as u64, Ordering::Relaxed);
        }
    }
}

fn downmix_and_resample_f32(
    data: &[f32],
    channels: usize,
    in_rate: u32,
    out_rate: u32,
) -> Vec<f32> {
    let channels = channels.max(1);
    let mut mono = Vec::with_capacity(data.len() / channels + 1);
    for frame in data.chunks(channels) {
        if frame.len() < channels {
            break;
        }
        let sum: f32 = frame.iter().sum();
        mono.push(sum / channels as f32);
    }
    resample_linear(&mono, in_rate, out_rate)
}

/// Se o nome é o microfone padrão do sistema (o que o SCK captura).
fn is_default_input(name: &str) -> bool {
    cpal::default_host()
        .default_input_device()
        .and_then(|d| d.name().ok())
        .is_some_and(|default| default == name)
}

/// Completa a faixa do microfone com silêncio até o tamanho da faixa do sistema.
/// A troca de microfone deixa uma pequena pausa; sem isso a voz ficaria
/// adiantada em relação à chamada na mixagem final.
fn pad_mic_to_system(shared: &CaptureShared) {
    let system = shared.system_samples.load(Ordering::Relaxed);
    let mic = shared.mic_samples.load(Ordering::Relaxed);
    let missing = system.saturating_sub(mic);
    // Mais de 10 s de diferença indica outro problema; não preenche às cegas.
    if missing == 0 || missing > SAMPLE_RATE as u64 * 10 {
        return;
    }
    if let Some(w) = shared.mic_writer.lock().as_mut() {
        for _ in 0..missing {
            let _ = w.write_sample(0i16);
        }
        shared.mic_samples.fetch_add(missing, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_mic_track_after_switching_microphone() {
        let dir = std::env::temp_dir().join(format!("rebobina-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mic_path = dir.join("mic.wav");
        let shared = CaptureShared {
            system_writer: Mutex::new(None),
            mic_writer: Mutex::new(Some(WavWriter::create(&mic_path, spec).unwrap())),
            sck_mic: AtomicBool::new(false),
            mic_peak: AtomicU64::new(0),
            system_peak: AtomicU64::new(0),
            mic_samples: AtomicU64::new(0),
            system_samples: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        };

        // Microfone gravou 1 s, sistema 1,5 s: a pausa da troca vira silêncio.
        write_i16_mono(&shared.mic_writer, &vec![0.1; 48_000], &shared.mic_peak, &shared.mic_samples);
        shared.system_samples.store(72_000, Ordering::Relaxed);
        pad_mic_to_system(&shared);
        assert_eq!(shared.mic_samples.load(Ordering::Relaxed), 72_000);

        // Diferença absurda (> 10 s) não é preenchida.
        shared.system_samples.store(72_000 + 48_000 * 20, Ordering::Relaxed);
        pad_mic_to_system(&shared);
        assert_eq!(shared.mic_samples.load(Ordering::Relaxed), 72_000);

        shared.mic_writer.lock().take().unwrap().finalize().unwrap();
        assert_eq!(hound::WavReader::open(&mic_path).unwrap().duration(), 72_000);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lists_input_devices_on_macos() {
        let devices = Recorder::list_input_devices().expect("deve listar devices");
        assert!(!devices.is_empty());
    }

    #[test]
    fn screen_permission_status_works() {
        let _ = screen_permission_status();
    }

    #[test]
    fn mix_planar_preserves_frame_count() {
        let left = vec![1.0f32, 0.5, -0.5];
        let right = vec![1.0f32, -0.5, 0.5];
        let mono = mix_planar_channels(&[left, right]);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 1.0).abs() < 1e-6);
        assert!((mono[1] - 0.0).abs() < 1e-6);
        assert!((mono[2] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn interleaved_style_wrong_downmix_halves_length() {
        // Regressão: tratar um canal planar como stereo interleaved
        // gerava metade das amostras (áudio “acelerado”).
        let planar_l = vec![0.1f32, 0.2, 0.3, 0.4];
        let wrong: Vec<f32> = planar_l
            .chunks(2)
            .map(|f| (f[0] + f[1]) / 2.0)
            .collect();
        assert_eq!(wrong.len(), 2);
        let correct = mix_planar_channels(&[planar_l]);
        assert_eq!(correct.len(), 4);
    }
}
