# bounce-house

Terminal multitrack recorder for band practice. Records from a multi-channel
audio interface, lets you mark and name takes while you play, and bounces
each named take to a stereo MP3 in the background.

## Goals

- Record a full band session with each channel on its own lossless track
- Mark and name takes by hand during play, without breaking flow
- Bounce each take to a shareable MP3 automatically
- Never drop a sample — the audio callback never blocks, the ring buffer
  has 10 seconds of headroom, and on-disk state is flushed every second

Built for my own band. Tested on macOS with CoreAudio; the rest of the
stack is portable but untested elsewhere.

## Build

Needs Rust (edition 2024) and a C compiler (LAME is built from source).

```sh
cargo run --release
```

Output goes to `./recordings/<timestamp>/`: one mono WAV per armed channel
plus a stereo MP3 per named take.

## Keys

| Mode | Key | Action |
|---|---|---|
| Idle | `R` | Start recording |
| Idle | `C` | Open channel picker |
| Idle | `W` | Cycle waveform window (10s / 30s / 1m / 5m / 30m) |
| Idle (post-stop) | `N` | Name the trailing unbound marker |
| Idle | `Q` / `Esc` | Quit |
| Recording | `T` | Drop a marker and name a take |
| Recording | `Space` | Drop an unbound marker |
| Recording | `N` | Name the last unbound marker as a take |
| Recording | `Backspace` | Delete the trailing unbound marker |
| Recording | `Esc` | Stop (press again to confirm) |
| Naming | `Enter` / `Esc` | Save / cancel |
| Channel picker | `↑↓` / `j`/`k` | Move cursor |
| Channel picker | `Space` | Toggle armed |
| Channel picker | `Tab` | Rename channel |
| Channel picker | `Esc` | Close |

The channel picker shows live meters for every input on the device,
armed or not — useful for identifying which physical input carries
which signal.

## Bouncing

Bouncing happens automatically when a take is named. The Recording
panel shows per-take status next to the duration. MP3s are 192 kbps
stereo, with all armed channels summed using `1/√N` scaling.

Bitrate, channel weighting, and output format are hardcoded for now.
Channel labels and arm states reset on every run. Both are tracked on
the roadmap.

## How it works

```
src/
├── main.rs           entry
├── app.rs            App + AppState (UI state machine)
├── audio/            cpal stream, ring buffer, per-channel WAV writer
├── bounce.rs         worker thread: hound → sum → LAME → mp3
├── recording.rs      one R-press-to-stop pass; owns the writer
├── timeline.rs       markers + takes + bounce status
├── session.rs        device + channels + output_dir
├── channel.rs        channel metadata
├── ui/               ratatui views
└── units.rs          newtypes
```

Four threads:

- **UI** — event loop, draws frames, mutates state
- **Audio callback** — cpal-managed; pushes raw frames to a 10s rtrb
- **Disk writer** — drains the rtrb into per-channel WAVs, flushes once
  per second to keep the WAV header current
- **Bounce worker** — receives jobs, waits for the take's end to be
  durable on disk, then streams chunks through hound → sum → LAME

No locks on the audio thread. Cross-thread state is `AtomicU64` (sample
position, flushed-samples), `AtomicF32` (per-channel peaks), and mpsc
for control + bounce jobs.

## Roadmap

- Sample-indexed level history (waveform is currently tick-indexed)
- Per-channel gain/pan applied during the bounce (currently a flat sum)
- Configurable bounce parameters
- Persistent per-device channel configuration
- Save/load a recording project

## Dependencies

[`cpal`](https://crates.io/crates/cpal),
[`ratatui`](https://crates.io/crates/ratatui),
[`crossterm`](https://crates.io/crates/crossterm),
[`rtrb`](https://crates.io/crates/rtrb),
[`hound`](https://crates.io/crates/hound),
[`mp3lame-encoder`](https://crates.io/crates/mp3lame-encoder) (vendors
LAME, statically linked, no system `libmp3lame` needed),
[`atomic_float`](https://crates.io/crates/atomic_float),
[`chrono`](https://crates.io/crates/chrono).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
