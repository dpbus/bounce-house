# bounce-house

Terminal multitrack recorder for band practice. Records from a multi-channel
audio interface, lets you mark and name takes while you play, and bounces
each named take to a stereo MP3 in the background.

## Goals

- Record a full band session with each channel on its own lossless track
- Mark and name takes by hand during play, without breaking flow
- Bounce each take to a shareable MP3 automatically, loudness-normalized
- Never drop a sample — the audio callback never blocks, the ring buffer
  has 10 seconds of headroom, and on-disk state is flushed every second

Built for my own band. Tested on macOS with CoreAudio; the rest of the
stack is portable but untested elsewhere.

## Build

Needs Rust (edition 2024) and a C compiler (LAME is built from source).

```sh
cargo run --release
```

## Output layout

```
~/Music/BounceHouse/
├── Sessions/<timestamp>/
│   ├── session.toml          metadata: tracks, markers, takes, bounce status
│   └── tracks/
│       └── ch07-Kick.wav     one mono WAV per armed channel
├── Bounces/<timestamp>/
│   └── <take>.mp3            one stereo MP3 per named take
└── Templates/<name>.toml     saved channel layouts

~/.config/bounce-house/config.toml   override the paths above
```

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
| Recording | `P` | Pause / resume |
| Recording | `Esc` | Stop (press again to confirm) |
| Naming | `Enter` / `Esc` | Save / cancel |
| Channel picker | `↑↓` / `j`/`k` | Move cursor |
| Channel picker | `Space` | Toggle armed |
| Channel picker | `Tab` | Rename channel |
| Channel picker | `Esc` | Close |

The channel picker shows live meters for every input on the device,
armed or not — useful for identifying which physical input carries
which signal.

## Templates

Channel labels and arm states can be saved as templates and loaded later.
Save via the in-app modal (writes to `Templates/<name>.toml`); load at
launch with `-t <name>` or `-t <path>`. Loading applies labels + arm
states to the current device's channels by index.

## Bouncing

Bouncing happens automatically when a take is named. The Recording panel
shows per-take status next to the duration. Each MP3 is 192 kbps stereo,
summed from all recorded tracks with `1/√N` scaling and **loudness-
normalized to -14 LUFS** (Spotify/YouTube target), with a `-1 dBTP`
true-peak ceiling so the gain stage never clips. Silent takes (below
-70 LUFS) skip normalization.

Bitrate, LUFS target, and channel weighting are hardcoded for now —
on the roadmap.

## How it works

```
src/
├── main.rs           entry, CLI parsing, optional template arg
├── app.rs            App: top-level runtime state (session, audio_input, capture, ...)
├── audio/            cpal input wrapper, ring buffer, per-track WAV writer
├── bounce.rs         worker thread: hound → sum → LUFS-normalize → LAME → mp3
├── capture.rs        runtime controller for an active recording (spawns/joins disk writer)
├── session.rs        Session: tracks + timeline + paths; persists to session.toml on mutation
├── track.rs          recorded artifact (one per channel armed at record-start)
├── channel.rs        live mixer-row config (index, label, armed)
├── timeline.rs       markers + takes + bounce status
├── template.rs       saved channel layouts
├── settings.rs       app-wide paths (sessions/, bounces/, templates/)
├── paths.rs          filesystem helpers
├── ui/               ratatui views, panels, modals, input handling
└── units.rs          newtypes
```

Four threads:

- **UI** — event loop, draws frames, mutates state
- **Audio callback** — cpal-managed; pushes raw frames into a 10s rtrb
- **Disk writer** — drains the rtrb into per-track WAVs, flushes once
  per second to keep the WAV header current
- **Bounce worker** — receives jobs, waits for the take's end to be
  durable on disk, then streams chunks through hound → sum →
  ebur128 (loudness measure) → LAME

No locks on the audio thread. Cross-thread state is `AtomicU64` (sample
position, flushed-samples), an rtrb of `LevelObservation` for per-channel
peaks, and mpsc for control + bounce jobs.

## Roadmap

- Sample-indexed level history (waveform is currently tick-indexed)
- Per-channel gain/pan applied during the bounce (currently a flat sum)
- Configurable bounce parameters (bitrate, LUFS target)
- Load existing sessions from disk (write is shipped; read isn't)
- Auto-recall last template per device

## Dependencies

[`cpal`](https://crates.io/crates/cpal),
[`ratatui`](https://crates.io/crates/ratatui),
[`crossterm`](https://crates.io/crates/crossterm),
[`rtrb`](https://crates.io/crates/rtrb),
[`hound`](https://crates.io/crates/hound),
[`mp3lame-encoder`](https://crates.io/crates/mp3lame-encoder) (vendors
LAME, statically linked, no system `libmp3lame` needed),
[`ebur128`](https://crates.io/crates/ebur128) (LUFS / true-peak),
[`clap`](https://crates.io/crates/clap),
[`serde`](https://crates.io/crates/serde) +
[`toml`](https://crates.io/crates/toml) (settings / session persistence),
[`uuid`](https://crates.io/crates/uuid),
[`palette`](https://crates.io/crates/palette),
[`chrono`](https://crates.io/crates/chrono),
[`rand`](https://crates.io/crates/rand).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
