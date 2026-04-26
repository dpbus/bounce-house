use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use hound::WavReader;
use mp3lame_encoder::{
    Bitrate, Builder, DualPcm, Encoder, FlushNoGap, Quality, max_required_buffer_size,
};

use crate::timeline::{BounceStatus, Take};
use crate::units::SampleRate;

type ChannelReader = WavReader<BufReader<File>>;

const CHUNK_SAMPLES: usize = 48_000;
const FLUSH_TAIL_BYTES: usize = 7200;

pub struct BounceJob {
    pub take: Take,
    pub sample_rate: SampleRate,
    pub output_dir: PathBuf,
    pub channel_files: Vec<PathBuf>,
    /// `None` when the recording has already stopped — the file is finalized
    /// and immediately readable. `Some` while live — bouncer waits on it.
    pub flushed_samples: Option<Arc<AtomicU64>>,
}

pub struct BounceUpdate {
    pub take_id: u32,
    pub status: BounceStatus,
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
            status: BounceStatus::Bouncing,
        });

        let status = match bounce_take(&job) {
            Ok(path) => BounceStatus::Done(path),
            Err(err) => BounceStatus::Failed(err),
        };
        let _ = updates.send(BounceUpdate {
            take_id: job.take.id,
            status,
        });
    }
}

fn wait_until_durable(job: &BounceJob) {
    let Some(flushed) = &job.flushed_samples else { return };
    while flushed.load(Ordering::Acquire) < job.take.end_sample {
        thread::sleep(Duration::from_millis(50));
    }
}

fn bounce_take(job: &BounceJob) -> Result<PathBuf, String> {
    if job.channel_files.is_empty() {
        return Err("no channel files".to_string());
    }
    let total = take_sample_count(&job.take)?;

    let readers = open_channel_readers(&job.channel_files, job.take.start_sample)?;
    let mut encoder = build_encoder(job.sample_rate)?;
    let path = unique_mp3_path(&job.output_dir, &job.take.name);
    let mut out_file =
        File::create(&path).map_err(|e| format!("create {}: {}", path.display(), e))?;

    encode_to_file(readers, total, &mut encoder, &mut out_file)?;

    Ok(path)
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
        let mut reader = WavReader::open(path)
            .map_err(|e| format!("open {}: {}", path.display(), e))?;
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
    encoder: &mut Encoder,
    out_file: &mut File,
) -> Result<(), String> {
    let scale = 1.0 / (readers.len() as f32).sqrt();
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
        .encode_to_vec(DualPcm { left: mono, right: mono }, mp3_out)
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

fn unique_mp3_path(dir: &Path, name: &str) -> PathBuf {
    let safe = safe_name(name);
    let base = dir.join(format!("{}.mp3", safe));
    if !base.exists() {
        return base;
    }
    for n in 2.. {
        let candidate = dir.join(format!("{}-{}.mp3", safe, n));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn safe_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "take".to_string();
    }
    trimmed
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
