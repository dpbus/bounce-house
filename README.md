# bounce-house

A terminal-based multitrack recorder for band practice. Captures audio from a
multi-channel interface, lets you arm and label channels live, mark and name
takes as you go, and writes one mono WAV per armed channel.

## Status

Active development. Capture, take marking, and review-after-stop all work end-
to-end on macOS with CoreAudio. Per-take MP3 bouncing is the next big feature.

## Building and running

Requires a recent stable Rust toolchain (edition 2024).

```sh
cargo run --release
```

Each recording goes to its own timestamped folder under `./recordings/`,
with one WAV per armed channel (`ch00-kick.wav`, `ch01-snare.wav`, …).

## Usage

When launched, the app:

1. Picks an audio device. If only one input device is available, it's
   auto-selected. Otherwise a small picker appears (`↑↓` to navigate,
   `Enter` to confirm).
2. Opens the main view: vertical meter strips for armed channels, a
   bipolar log-scale waveform, recording-info panel, and a contextual key
   hint footer.

### Keys

| Mode | Key | Action |
|---|---|---|
| Idle | `R` | Start recording (requires at least one armed channel) |
| Idle | `C` | Open channel picker modal |
| Idle | `W` | Cycle waveform window size (10s / 30s / 1m / 5m / 30m) |
| Idle (post-recording) | `N` | Name the trailing unbound marker as a take |
| Idle | `Q` / `Esc` | Quit |
| Recording | `T` | Drop a marker and start naming a take |
| Recording | `Space` | Drop an unbound boundary marker |
| Recording | `N` | Name the last unbound marker as a take |
| Recording | `Backspace` | Delete the trailing unbound marker |
| Recording | `W` | Cycle waveform window size |
| Recording | `Esc` | Confirm stop (press again to actually stop) |
| Naming take | `Enter` | Save name |
| Naming take | `Esc` | Cancel (T also rolls back the marker; N just closes) |
| Channel picker | `↑↓` / `j`/`k` | Move cursor |
| Channel picker | `Space` | Toggle armed |
| Channel picker | `Tab` | Rename current channel |
| Channel picker | `Esc` | Close picker |
| Renaming | `Enter` | Save name |
| Renaming | `Esc` | Cancel |

Channel picker shows live meters for every input on the device, including
unarmed ones — useful for identifying which physical input carries which
signal.

After stopping a recording, the panel and waveform markers stay visible
until the next `R` press starts a fresh recording. You can still press
`N` post-stop to label the trailing span as a take.

## Architecture

```
src/
├── main.rs                 entry
├── app.rs                  App + AppState (UI state machine)
├── channel.rs              Channel (metadata: index, label, armed)
├── session.rs              Session (channels, raw_dir, started_at)
├── recording.rs            Recording (started_at, stopped_at, output_dir, timeline)
├── timeline.rs             Timeline (markers + takes — pure data)
├── units.rs                newtypes (SampleRate, SamplePosition, ChannelIndex)
├── audio/                  audio runtime — cpal contained here
│   ├── device.rs           cpal device wrapper
│   ├── engine.rs           single persistent stream, lock-free command queue
│   ├── levels.rs           ChannelLevel (atomic peak)
│   └── disk_writer.rs      RAII writer thread + WAV finalization on drop
└── ui/
    ├── mod.rs              event loop, key dispatch
    ├── device_picker.rs    boot-phase device selection
    ├── main_view.rs        top-level layout, outer frame, footer hints
    ├── session_panel.rs    top-left panel — device, started, channels, output
    ├── recording_panel.rs  top-right panel — timer, folder, takes (multi-column)
    ├── waveform.rs         bipolar dB-scaled waveform with marker glyphs
    ├── meter_panel.rs      armed channel strips
    ├── channel_picker.rs   modal overlay for arming and renaming channels
    └── widgets.rs          shared chrome (panel, meters, key hints, columns)
```

### Domain model

- **Session** = app-level config (device, channels, output_dir). One per app run.
- **Recording** = one R-press-to-stop capture pass. Owns its `Timeline`. A
  session may contain many recordings sequentially; only the current/most-
  recent one is held in-memory (replaced on the next `R` press).
- **Timeline** = pure data: markers + takes + color counter, no UI flow state.
- **Marker** = `{ tick }`, auto-pushed on start/stop or by Space/T.
- **Take** = `{ name, start_tick, end_tick, color_index }`, created when a
  naming overlay commits.

`App` has three orthogonal axes:

```rust
pub recording: Option<Recording>,    // domain — current/last recording
pub writer: Option<DiskWriter>,      // resource — Some only while writing to disk
pub state: AppState,                 // pure UI state machine
```

`AppState` is `Default` / `NamingTake { buf, origin }` / `ConfirmingStop` /
`PickingChannel { … }` — modes only, no embedded data.

### Threading

- **UI thread**: event loop, reads atomic levels and sample position, mutates
  session/recording/state via App methods.
- **Audio callback thread**: cpal-managed, high-priority. Reads command
  queue, publishes per-channel peaks, pushes raw frames to an rtrb ring
  buffer while a recording is active.
- **Disk writer thread**: spawned per recording. Reads from the ring buffer,
  picks armed channels, writes per-channel WAVs. Exits and finalizes on drop
  of the `DiskWriter` handle.

No mutexes are taken on the audio thread. UI ↔ audio communication uses
atomics for level/position state and an mpsc channel for control messages.

## Dependencies

- [`cpal`](https://crates.io/crates/cpal) — cross-platform audio I/O
- [`ratatui`](https://crates.io/crates/ratatui) + [`crossterm`](https://crates.io/crates/crossterm) — terminal UI
- [`rtrb`](https://crates.io/crates/rtrb) — lock-free single-producer/single-consumer ring buffer
- [`hound`](https://crates.io/crates/hound) — WAV writer
- [`atomic_float`](https://crates.io/crates/atomic_float) — `AtomicF32` for peak meters
- [`chrono`](https://crates.io/crates/chrono) — local time for filenames

## Roadmap

- Per-take stereo mixdown to MP3 via ffmpeg
- Save/load a recording project (the `Recording` struct is structured to
  serialize cleanly)
- Marker selection mode for retroactively naming earlier spans (currently
  N only acts on the literal last marker)
- Persistent per-device channel configuration (names, armed defaults)
- Configurable recording output path

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
