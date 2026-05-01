//! Per-channel envelope-modulated noise driving the fake device.
//!
//! Each channel gets a unique envelope rate, phase offset, amplitude
//! range, and ramp shape so meters peak at different times and with
//! different rhythms — some channels gradually swell, others spike
//! sharply, all sit silent in between.

use rand::Rng;
use rand::rngs::ThreadRng;

pub(super) struct Signal {
    total_channels: usize,
    sample_rate: u32,
    env_params: Vec<EnvParams>,
    envs: Vec<f32>,
    sample_clock: u64,
    rng: ThreadRng,
}

struct EnvParams {
    rate: f32,
    offset: f32,
    min: f32,
    max: f32,
    /// Exponent applied to the active half of the envelope cycle.
    /// Low (~1) gives a gentle swell; high (~5) gives a sharp spike.
    shape: f32,
}

impl Signal {
    pub(super) fn new(total_channels: usize, sample_rate: u32) -> Self {
        let env_params = (0..total_channels).map(channel_env_params).collect();
        Self {
            total_channels,
            sample_rate,
            env_params,
            envs: vec![0.0; total_channels],
            sample_clock: 0,
            rng: rand::thread_rng(),
        }
    }

    pub(super) fn fill(&mut self, buffer: &mut [f32]) {
        let frames = buffer.len() / self.total_channels;
        let t = self.sample_clock as f32 / self.sample_rate as f32;
        for (ch, p) in self.env_params.iter().enumerate() {
            let phase = std::f32::consts::TAU * p.rate * t + p.offset;
            // Half-wave gate: when sin <= 0 the envelope sits at `min`
            // (true silence). The other half is raised to `shape` so
            // some channels swell gradually and others spike sharply.
            let active = phase.sin().max(0.0);
            let skewed = active.powf(p.shape);
            self.envs[ch] = p.min + (p.max - p.min) * skewed;
        }
        for frame_idx in 0..frames {
            for ch in 0..self.total_channels {
                buffer[frame_idx * self.total_channels + ch] =
                    self.rng.gen_range(-self.envs[ch]..self.envs[ch]);
            }
        }
        self.sample_clock += frames as u64;
    }
}

fn channel_env_params(ch: usize) -> EnvParams {
    let c = ch as f32;
    EnvParams {
        rate: 0.2 + (c * 0.31).sin().abs() * 1.6,
        offset: c * 0.7,
        min: 0.005 + (c * 0.7).sin().abs() * 0.03,
        max: 0.30 + (c * 1.13).sin().abs() * 0.60,
        shape: 1.0 + (c * 1.7).sin().abs() * 4.0,
    }
}
