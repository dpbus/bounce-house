use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use ebur128::{EbuR128, Mode};
use hound::WavReader;
use mp3lame_encoder::{
    Bitrate, Builder, DualPcm, Encoder, FlushNoGap, Quality, max_required_buffer_size,
};
use uuid::Uuid;

use crate::timeline::Take;
use crate::units::SampleRate;

type ChannelReader = WavReader<BufReader<File>>;

const CHUNK_SAMPLES: usize = 48_000;
const FLUSH_TAIL_BYTES: usize = 7200;

/// Streaming-era integrated loudness target. Spotify/YouTube ≈ -14, Apple ≈ -16.
const TARGET_LUFS: f64 = -14.0;
/// True-peak ceiling after gain — gain caps below this so we never clip.
const MAX_TRUE_PEAK_DB: f64 = -1.0;
/// Below this LUFS the take is effectively silent; skip normalization.
const SILENCE_LUFS_FLOOR: f64 = -70.0;

pub struct BounceJob {
    pub take: Take,
    pub sample_rate: SampleRate,
    pub bounces_dir: PathBuf,
    /// Prepended to the take's filename when present — used in flat
    /// layouts where multiple sessions share one bounces dir. None for
    /// nested layouts where `bounces_dir` is already session-specific.
    pub filename_prefix: Option<String>,
    pub channel_files: Vec<PathBuf>,
    /// `None` if the recording has already stopped (file is finalized,
    /// immediately readable). `Some` while live; bouncer waits on it.
    pub flushed_samples: Option<Arc<AtomicU64>>,
}

pub enum BounceEvent {
    Started,
    Done(PathBuf),
    Failed,
}

pub struct BounceUpdate {
    pub take_id: Uuid,
    pub event: BounceEvent,
}

pub struct BouncePool {
    job_tx: Sender<BounceJob>,
    update_rx: Receiver<BounceUpdate>,
}

impl BouncePool {
    pub fn start() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<BounceJob>();
        let (update_tx, update_rx) = mpsc::channel::<BounceUpdate>();
        thread::spawn(move || worker_loop(job_rx, update_tx));
        BouncePool { job_tx, update_rx }
    }

    pub fn dispatch(&self, job: BounceJob) {
        let _ = self.job_tx.send(job);
    }

    pub fn drain_updates(&self) -> Vec<BounceUpdate> {
        let mut out = Vec::new();
        while let Ok(update) = self.update_rx.try_recv() {
            out.push(update);
        }
        out
    }
}

fn worker_loop(jobs: Receiver<BounceJob>, updates: Sender<BounceUpdate>) {
    for job in jobs {
        wait_until_durable(&job);

        let _ = updates.send(BounceUpdate {
            take_id: job.take.id,
            event: BounceEvent::Started,
        });

        let event = match bounce_take(&job) {
            Ok(path) => BounceEvent::Done(path),
            Err(_) => BounceEvent::Failed,
        };
        let _ = updates.send(BounceUpdate {
            take_id: job.take.id,
            event,
        });
    }
}

fn wait_until_durable(job: &BounceJob) {
    let Some(flushed) = &job.flushed_samples else {
        return;
    };
    while flushed.load(Ordering::Acquire) < job.take.end_sample {
        thread::sleep(Duration::from_millis(50));
    }
}

fn bounce_take(job: &BounceJob) -> Result<PathBuf, String> {
    if job.channel_files.is_empty() {
        return Err("no channel files".to_string());
    }
    fs::create_dir_all(&job.bounces_dir)
        .map_err(|e| format!("create dir {}: {}", job.bounces_dir.display(), e))?;
    let total = take_sample_count(&job.take)?;

    let (lufs, true_peak) = analyze_loudness(job, total)?;
    let gain = compute_normalization_gain(lufs, true_peak);

    let readers = open_channel_readers(&job.channel_files, job.take.start_sample)?;
    let mut encoder = build_encoder(job.sample_rate)?;
    let path = crate::paths::unique_mp3_path(
        &job.bounces_dir,
        job.filename_prefix.as_deref(),
        &job.take.name,
    );
    let mut out_file =
        File::create(&path).map_err(|e| format!("create {}: {}", path.display(), e))?;

    encode_to_file(readers, total, gain, &mut encoder, &mut out_file)?;

    Ok(path)
}

/// First pass over the take: sums per-channel WAVs to mono and feeds
/// the result to ebur128 as stereo (L=R), matching what the encode
/// pass produces. Returns integrated LUFS and the max true-peak
/// across L/R as a linear amplitude.
fn analyze_loudness(job: &BounceJob, total: usize) -> Result<(f64, f64), String> {
    let mut readers = open_channel_readers(&job.channel_files, job.take.start_sample)?;
    let mut analyzer = EbuR128::new(2, job.sample_rate.0, Mode::I | Mode::TRUE_PEAK)
        .map_err(|e| format!("ebur128 init: {:?}", e))?;

    let scale = 1.0 / (readers.len() as f32).sqrt();
    let mut mono = vec![0.0f32; CHUNK_SAMPLES];
    let mut interleaved = vec![0.0f32; CHUNK_SAMPLES * 2];
    let mut done = 0usize;
    while done < total {
        let chunk = (total - done).min(CHUNK_SAMPLES);
        let n = mix_chunk_into(&mut readers, &mut mono[..chunk], scale);
        if n == 0 {
            break;
        }
        // The encoder writes stereo with L=R; mirror that here so the
        // LUFS we measure matches the signal we're about to encode.
        for (i, &s) in mono[..n].iter().enumerate() {
            interleaved[i * 2] = s;
            interleaved[i * 2 + 1] = s;
        }
        analyzer
            .add_frames_f32(&interleaved[..n * 2])
            .map_err(|e| format!("ebur128 add_frames: {:?}", e))?;
        done += n;
        if n < chunk {
            break;
        }
    }

    let lufs = analyzer
        .loudness_global()
        .map_err(|e| format!("ebur128 loudness_global: {:?}", e))?;
    let tp_l = analyzer
        .true_peak(0)
        .map_err(|e| format!("ebur128 true_peak L: {:?}", e))?;
    let tp_r = analyzer
        .true_peak(1)
        .map_err(|e| format!("ebur128 true_peak R: {:?}", e))?;
    Ok((lufs, tp_l.max(tp_r)))
}

/// Linear gain factor that brings `lufs` toward `TARGET_LUFS` without
/// pushing `true_peak` above `MAX_TRUE_PEAK_DB`. Returns 1.0 (pass-through)
/// for silent or near-silent takes.
fn compute_normalization_gain(lufs: f64, true_peak: f64) -> f32 {
    if !lufs.is_finite() || lufs < SILENCE_LUFS_FLOOR {
        return 1.0;
    }
    let ideal_db = TARGET_LUFS - lufs;
    let peak_db = if true_peak > 0.0 {
        20.0 * true_peak.log10()
    } else {
        f64::NEG_INFINITY
    };
    let max_safe_db = MAX_TRUE_PEAK_DB - peak_db;
    let gain_db = ideal_db.min(max_safe_db);
    10f32.powf(gain_db as f32 / 20.0)
}

fn take_sample_count(take: &Take) -> Result<usize, String> {
    let total = take
        .end_sample
        .checked_sub(take.start_sample)
        .ok_or_else(|| "take has end before start".to_string())? as usize;
    if total == 0 {
        return Err("take is empty".to_string());
    }
    Ok(total)
}

fn open_channel_readers(
    paths: &[PathBuf],
    start_sample: u64,
) -> Result<Vec<ChannelReader>, String> {
    let mut readers = Vec::with_capacity(paths.len());
    for path in paths {
        let mut reader =
            WavReader::open(path).map_err(|e| format!("open {}: {}", path.display(), e))?;
        reader
            .seek(start_sample as u32)
            .map_err(|e| format!("seek {}: {}", path.display(), e))?;
        readers.push(reader);
    }
    Ok(readers)
}

fn build_encoder(sample_rate: SampleRate) -> Result<Encoder, String> {
    let mut builder = Builder::new().ok_or_else(|| "lame builder init failed".to_string())?;
    builder
        .set_sample_rate(sample_rate.0)
        .map_err(|e| format!("lame sample_rate: {:?}", e))?;
    builder
        .set_num_channels(2)
        .map_err(|e| format!("lame channels: {:?}", e))?;
    builder
        .set_brate(Bitrate::Kbps192)
        .map_err(|e| format!("lame brate: {:?}", e))?;
    builder
        .set_quality(Quality::Best)
        .map_err(|e| format!("lame quality: {:?}", e))?;
    builder.build().map_err(|e| format!("lame build: {:?}", e))
}

fn encode_to_file(
    mut readers: Vec<ChannelReader>,
    total: usize,
    gain: f32,
    encoder: &mut Encoder,
    out_file: &mut File,
) -> Result<(), String> {
    let scale = (1.0 / (readers.len() as f32).sqrt()) * gain;
    let mut mono = vec![0.0f32; CHUNK_SAMPLES];
    let mut mp3_out: Vec<u8> = Vec::with_capacity(max_required_buffer_size(CHUNK_SAMPLES));

    let mut done = 0usize;
    while done < total {
        let chunk = (total - done).min(CHUNK_SAMPLES);
        let n = mix_chunk_into(&mut readers, &mut mono[..chunk], scale);
        if n == 0 {
            break;
        }
        encode_chunk(encoder, &mono[..n], &mut mp3_out, out_file)?;
        done += n;
        if n < chunk {
            break;
        }
    }

    encode_tail(encoder, &mut mp3_out, out_file)
}

/// Reads up to `dst.len()` samples from each reader, sums into `dst`, scales.
/// Returns the count actually written (limited by the shortest channel read).
fn mix_chunk_into(readers: &mut [ChannelReader], dst: &mut [f32], scale: f32) -> usize {
    dst.fill(0.0);
    let mut min_read = dst.len();
    for reader in readers.iter_mut() {
        let mut samples = reader.samples::<f32>();
        let mut count = 0;
        for slot in dst.iter_mut() {
            let Some(Ok(s)) = samples.next() else { break };
            *slot += s;
            count += 1;
        }
        min_read = min_read.min(count);
    }
    for s in &mut dst[..min_read] {
        *s *= scale;
    }
    min_read
}

fn encode_chunk(
    encoder: &mut Encoder,
    mono: &[f32],
    mp3_out: &mut Vec<u8>,
    out_file: &mut File,
) -> Result<(), String> {
    mp3_out.clear();
    mp3_out.reserve(max_required_buffer_size(mono.len()));
    encoder
        .encode_to_vec(
            DualPcm {
                left: mono,
                right: mono,
            },
            mp3_out,
        )
        .map_err(|e| format!("encode: {:?}", e))?;
    out_file
        .write_all(mp3_out)
        .map_err(|e| format!("write: {}", e))
}

fn encode_tail(
    encoder: &mut Encoder,
    mp3_out: &mut Vec<u8>,
    out_file: &mut File,
) -> Result<(), String> {
    mp3_out.clear();
    mp3_out.reserve(FLUSH_TAIL_BYTES);
    encoder
        .flush_to_vec::<FlushNoGap>(mp3_out)
        .map_err(|e| format!("flush: {:?}", e))?;
    out_file
        .write_all(mp3_out)
        .map_err(|e| format!("write tail: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::BounceStatus;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::path::Path;
    use tempfile::tempdir;

    fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) {
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec).expect("create wav");
        for &s in samples {
            writer.write_sample(s).expect("write sample");
        }
        writer.finalize().expect("finalize wav");
    }

    fn fake_take(start: u64, end: u64) -> Take {
        Take {
            id: Uuid::new_v4(),
            name: "test".into(),
            start_sample: start,
            end_sample: end,
            color_index: 0,
            bounce_path: None,
            bounce_status: BounceStatus::Pending,
        }
    }

    #[test]
    fn compute_gain_silence_returns_unity() {
        // Below the silence floor → no normalization applied.
        assert_eq!(compute_normalization_gain(-80.0, 0.001), 1.0);
        assert_eq!(compute_normalization_gain(f64::NEG_INFINITY, 0.0), 1.0);
        assert_eq!(compute_normalization_gain(f64::NAN, 0.5), 1.0);
    }

    #[test]
    fn compute_gain_brings_loud_signal_down_to_target() {
        // Source at -10 LUFS, target -14 → need -4 dB → gain ≈ 0.631.
        let gain = compute_normalization_gain(-10.0, 0.5);
        assert!((gain - 0.631).abs() < 0.005, "expected ~0.631, got {gain}");
    }

    #[test]
    fn compute_gain_brings_quiet_signal_up_to_target() {
        // Source at -20 LUFS, low peak → need +6 dB → gain ≈ 1.995.
        // Peak at 0.1 (-20 dBTP), so peak ceiling is -1 - (-20) = +19 dB,
        // which exceeds the +6 dB needed for LUFS — LUFS path wins.
        let gain = compute_normalization_gain(-20.0, 0.1);
        assert!((gain - 1.995).abs() < 0.01, "expected ~1.995, got {gain}");
    }

    #[test]
    fn compute_gain_caps_at_peak_ceiling_to_avoid_clipping() {
        // Source at -30 LUFS (wants +16 dB) but peak already at 0.9 (-0.92 dBTP).
        // Peak ceiling allows only -1 - (-0.92) = -0.08 dB, so gain ≈ 0.99.
        let gain = compute_normalization_gain(-30.0, 0.9);
        assert!(gain < 1.0, "peak limit should reduce, got {gain}");
        assert!(gain > 0.95, "shouldn't reduce dramatically, got {gain}");
    }

    #[test]
    fn take_sample_count_returns_difference() {
        let take = fake_take(100, 1000);
        assert_eq!(take_sample_count(&take).unwrap(), 900);
    }

    #[test]
    fn take_sample_count_rejects_zero_length() {
        let take = fake_take(500, 500);
        assert!(take_sample_count(&take).is_err());
    }

    #[test]
    fn take_sample_count_rejects_inverted_range() {
        let take = fake_take(1000, 500);
        assert!(take_sample_count(&take).is_err());
    }

    #[test]
    fn bounce_take_writes_an_mp3_file() {
        // Smoke-test the full encode path: two channel WAVs, a take that
        // covers their full extent, run bounce_take, verify the MP3 exists
        // and is non-empty. Doesn't decode — that's a heavier test.
        let dir = tempdir().unwrap();
        let bounces_dir = dir.path().join("bounces");
        std::fs::create_dir_all(&bounces_dir).unwrap();

        let ch0 = dir.path().join("ch0.wav");
        let ch1 = dir.path().join("ch1.wav");
        // 1 second of silence at 48kHz so ebur128 reaches "silent" floor.
        let samples: Vec<f32> = vec![0.0; 48_000];
        write_wav(&ch0, &samples, 48_000);
        write_wav(&ch1, &samples, 48_000);

        let job = BounceJob {
            take: fake_take(0, 48_000),
            sample_rate: SampleRate(48_000),
            bounces_dir: bounces_dir.clone(),
            filename_prefix: None,
            channel_files: vec![ch0, ch1],
            flushed_samples: None,
        };

        let result = bounce_take(&job);
        let path = result.expect("bounce_take should succeed");
        assert!(path.exists(), "mp3 was not written");
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size > 0, "mp3 file is empty");
    }

    #[test]
    fn bounce_take_creates_bounces_dir_if_missing() {
        // Regression: regular bug was that per-session bounces_dir wasn't
        // being created lazily. Confirm bounce_take creates it.
        let dir = tempdir().unwrap();
        let bounces_dir = dir.path().join("nested").join("missing").join("dir");
        assert!(!bounces_dir.exists());

        let ch0 = dir.path().join("ch0.wav");
        let samples: Vec<f32> = vec![0.0; 48_000];
        write_wav(&ch0, &samples, 48_000);

        let job = BounceJob {
            take: fake_take(0, 48_000),
            sample_rate: SampleRate(48_000),
            bounces_dir: bounces_dir.clone(),
            filename_prefix: None,
            channel_files: vec![ch0],
            flushed_samples: None,
        };

        bounce_take(&job).expect("bounce_take should succeed");
        assert!(bounces_dir.exists(), "bounces_dir was not created");
    }
}
