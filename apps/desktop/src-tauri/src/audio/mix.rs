use crate::error::{AppError, AppResult};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::Path;

/// Mistura dois WAVs mono/PCM em um único mono 16-bit (média das amostras alinhadas).
pub fn mix_mono_wavs(system_path: &Path, mic_path: &Path, out_path: &Path) -> AppResult<()> {
    let sys_meta = std::fs::metadata(system_path).ok();
    let mic_meta = std::fs::metadata(mic_path).ok();

    let sys_ok = sys_meta.as_ref().map(|m| m.len() > 44).unwrap_or(false);
    let mic_ok = mic_meta.as_ref().map(|m| m.len() > 44).unwrap_or(false);

    if !sys_ok && !mic_ok {
        return Err(AppError::Audio(
            "Gravação vazia — sem áudio do sistema nem do microfone".into(),
        ));
    }

    if sys_ok && !mic_ok {
        std::fs::copy(system_path, out_path)?;
        return Ok(());
    }
    if mic_ok && !sys_ok {
        std::fs::copy(mic_path, out_path)?;
        return Ok(());
    }

    let mut sys = WavReader::open(system_path).map_err(|e| AppError::Audio(e.to_string()))?;
    let mut mic = WavReader::open(mic_path).map_err(|e| AppError::Audio(e.to_string()))?;

    let sys_spec = sys.spec();
    let mic_spec = mic.spec();
    let sample_rate = sys_spec.sample_rate.max(mic_spec.sample_rate);

    let sys_samples = read_as_f32(&mut sys)?;
    let mic_samples = read_as_f32(&mut mic)?;

    // Volume de cada faixa ao longo do tempo, para saber depois quem falou.
    // Falha aqui não deve impedir a gravação de ser salva.
    let levels = crate::speakers::SpeakerLevels::from_tracks(
        &mic_samples,
        mic_spec.sample_rate,
        &sys_samples,
        sys_spec.sample_rate,
    );
    if let Err(e) = levels.save(&crate::speakers::SpeakerLevels::path_for(out_path)) {
        eprintln!("rebobina: não foi possível salvar níveis por faixa: {e}");
    }

    let len = sys_samples.len().max(mic_samples.len());
    let mut mixed = Vec::with_capacity(len);
    for i in 0..len {
        let s = *sys_samples.get(i).unwrap_or(&0.0);
        let m = *mic_samples.get(i).unwrap_or(&0.0);
        // Soma com leve atenuação para evitar clipping
        mixed.push((s + m) * 0.7);
    }

    let mut writer = WavWriter::create(
        out_path,
        WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| AppError::Audio(e.to_string()))?;

    for s in mixed {
        let i = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(i)
            .map_err(|e| AppError::Audio(e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| AppError::Audio(e.to_string()))?;
    Ok(())
}

fn read_as_f32(reader: &mut WavReader<std::io::BufReader<std::fs::File>>) -> AppResult<Vec<f32>> {
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let mut mono = Vec::new();

    match spec.sample_format {
        SampleFormat::Int => {
            let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
            let samples = samples.map_err(|e| AppError::Audio(e.to_string()))?;
            for frame in samples.chunks(channels) {
                let sum: f32 = frame.iter().map(|&s| s as f32 / i16::MAX as f32).sum();
                mono.push(sum / channels as f32);
            }
        }
        SampleFormat::Float => {
            let samples: Result<Vec<f32>, _> = reader.samples::<f32>().collect();
            let samples = samples.map_err(|e| AppError::Audio(e.to_string()))?;
            for frame in samples.chunks(channels) {
                let sum: f32 = frame.iter().sum();
                mono.push(sum / channels as f32);
            }
        }
    }
    Ok(mono)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavSpec;

    #[test]
    fn mixes_two_wavs() {
        let dir = std::env::temp_dir().join(format!("rebobina-mix-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        let out = dir.join("out.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut w = WavWriter::create(&a, spec).unwrap();
            for _ in 0..800 {
                w.write_sample(1000i16).unwrap();
            }
            w.finalize().unwrap();
        }
        {
            let mut w = WavWriter::create(&b, spec).unwrap();
            for _ in 0..800 {
                w.write_sample(2000i16).unwrap();
            }
            w.finalize().unwrap();
        }

        mix_mono_wavs(&a, &b, &out).unwrap();
        assert!(out.metadata().unwrap().len() > 44);
        let _ = std::fs::remove_dir_all(dir);
    }
}
