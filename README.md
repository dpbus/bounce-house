# bounce-house

A terminal-based multitrack recorder for band practice. Captures audio from a
multi-channel interface, lets you arm and label channels live, and writes
each session to a multi-channel WAV file.

## Status

Active development. Recording works end-to-end on macOS with CoreAudio. Take
marking and per-take MP3 bouncing are planned but not implemented.

## Building and running

Requires a recent stable Rust toolchain (edition 2024).

```sh
cargo run --release
```

WAV files are written to `./recordings/` in the working directory.

## Usage

When launched, the app:

1. Picks an audio device. If only one input device is available, it's
   auto-selected. Otherwise a small picker appears (`↑↓` to navigate,
   `Enter` to confirm).
2. Opens the main view: vertical meter strips for armed channels (none
   initially), a status line, and a contextual key hint footer.

### Keys

| Mode | Key | Action |
|---|---|---|
| Idle | `R` | Start recording (requires at least one armed channel) |
| Idle | `C` | Open channel picker modal |
| Idle | `Q` / `Esc` | Quit |
| Recording | `Esc` | Confirm stop (press again to actually stop) |
| Channel picker | `↑↓` / `j`/`k` | Move cursor |
| Channel picker | `Space` | Toggle armed |
| Channel picker | `Tab` | Rename current channel |
| Channel picker | `Esc` | Close picker |
| Renaming | `Enter` | Save name |
| Renaming | `Esc` | Cancel |

Channel picker shows live meters for every input on the device, including
unarmed ones — useful for identifying which physical input carries which
signal.

## Architecture

The codebase splits into three layers:

```
src/
├── main.rs                 entry
├── app.rs                  App + AppState (state machine)
├── channel.rs              Channel (metadata: index, label, armed)
├── session.rs              Session (channels, raw_dir, started_at)
├── units.rs                newtypes (SampleRate, SamplePosition, ChannelIndex)
├── audio/                  audio runtime — cpal contained here
│   ├── device.rs           cpal device wrapper, public metadata + sealed stream builder
│   ├── engine.rs           single persistent stream, lock-free command queue
│   ├── levels.rs           ChannelLevel (atomic peak)
│   └── recording.rs        RAII writer thread + WAV finalization on drop
└── ui/
    ├── device_picker.rs    boot-phase device selection
    ├── main_view.rs        primary screen — vertical meter strips
    ├── channel_picker.rs   modal overlay for arming and renaming channels
    ├── widgets.rs          shared meter and key-hint helpers
    └── mod.rs              event loop, key dispatch
```

### Threading

- **UI thread**: runs the event loop, reads atomic levels and sample
  position, mutates session/channel/state via App methods.
- **Audio callback thread**: cpal-managed, high-priority. Reads command
  queue, publishes per-channel peaks, pushes raw frames to an rtrb ring
  buffer when recording.
- **Disk writer thread**: spawned per recording. Reads from the ring
  buffer, picks armed channels, writes the WAV. Exits and finalizes on
  drop of the `Recording` handle.

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

- Take marking during recording (mark song boundaries with a hotkey)
- Per-take stereo mixdown to MP3 via ffmpeg
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
