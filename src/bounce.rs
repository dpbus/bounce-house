use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::audio::RECORDING_BUFFER_SECONDS;
use crate::timeline::{BounceStatus, Take};
use crate::units::SampleRate;

pub struct BounceJob {
    pub take: Take,
    pub sample_rate: SampleRate,
    pub output_dir: PathBuf,
    pub channel_files: Vec<PathBuf>,
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
        // Wait long enough that any in-flight rtrb samples + BufWriter
        // contents are provably durable on disk. The rtrb's capacity is
        // the writer-lag failure threshold — if we'd ever wait longer than
        // this, we'd already be dropping samples at the audio thread.
        thread::sleep(Duration::from_secs(RECORDING_BUFFER_SECONDS as u64 + 1));

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

fn bounce_take(_job: &BounceJob) -> Result<PathBuf, String> {
    Err("not yet implemented".to_string())
}
