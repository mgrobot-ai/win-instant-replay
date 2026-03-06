# Win Instant Replay

A small Windows-only Rust tray app that keeps a rolling, disk-backed screen recording buffer and saves the last **10s / 30s / 60s / 120s / 300s** on demand via global hotkeys.

## What it does

- runs quietly in the background
- captures the desktop continuously with **ffmpeg**
- writes short encoded MP4 segments to disk instead of storing raw frames in RAM
- keeps only enough temporary segments to cover the configured replay window
- saves a timestamped MP4 when a configured hotkey is pressed
- provides a tray icon with:
  - **Open Output Folder**
  - **Quit**

## Why this design

The main goal here is low memory usage and simple, robust behavior.

Instead of building a heavyweight recorder in Rust, the Rust app acts as a controller:

1. it launches `ffmpeg` with Windows desktop capture (`gdigrab`)
2. `ffmpeg` writes 1-second MP4 segments into a temp buffer directory
3. the Rust app deletes old segments beyond the configured retention window
4. when you press a hotkey, the Rust app concatenates the newest completed segments into a replay clip

That keeps the Rust process lightweight and avoids an in-memory raw-frame ring buffer.

## Current scope

This implementation records **screen only** by default.

Audio capture is intentionally not enabled in the default setup because Windows audio input/device selection gets messy fast and varies a lot across machines. The structure leaves room to extend the ffmpeg command later if you want microphone/system audio.

## Requirements

- Windows 10 or 11
- Rust toolchain for building
- `ffmpeg.exe` available on `PATH`, or configured explicitly in `config.toml`
- an ffmpeg build with:
  - `gdigrab`
  - `libx264`

## Build

From a Windows shell in this project directory:

```powershell
cargo build --release
```

Release builds use the Windows subsystem flag, so the app should not open a console window.

The binary will be:

```text
target\release\win-instant-replay.exe
```

## Run

Just launch the executable:

```powershell
.\target\release\win-instant-replay.exe
```

On first run it will create a default config file if one does not exist.

## Default behavior

- segment size: **1 second**
- max replay window: **300 seconds**
- frame rate: **30 fps**
- encoder: `libx264`
- preset: `veryfast`
- input: `desktop` via `gdigrab`

Default hotkeys:

- `Ctrl+Alt+Shift+1` → save last 10 seconds
- `Ctrl+Alt+Shift+2` → save last 30 seconds
- `Ctrl+Alt+Shift+3` → save last 60 seconds
- `Ctrl+Alt+Shift+4` → save last 120 seconds
- `Ctrl+Alt+Shift+5` → save last 300 seconds

## Config

See [`config.example.toml`](./config.example.toml).

The app also auto-generates a default `config.toml` in its app config directory on first launch.

Config fields:

- `ffmpeg_path` - optional explicit path to `ffmpeg.exe`
- `buffer_dir` - optional override for the rolling temp segment directory
- `output_dir` - optional override for saved replay clips
- `max_replay_seconds` - maximum rolling history to keep
- `segment_seconds` - segment size; default is `1`
- `frame_rate` - capture fps
- `encoder` - ffmpeg video encoder, default `libx264`
- `preset` - ffmpeg preset, default `veryfast`
- `ffmpeg_input` - ffmpeg capture input, default `desktop`
- `ffmpeg_extra_args` - extra args inserted before segment output options
- `hotkeys` - replay durations and key combos

### Supported hotkey format

Examples:

- `Ctrl+Alt+Shift+1`
- `Ctrl+Alt+F10`
- `Win+Shift+R`

Supported modifiers:

- `Ctrl` / `Control`
- `Alt`
- `Shift`
- `Win` / `Meta` / `Super`

Supported keys:

- `0`-`9`
- `A`-`Z`
- `F1`-`F24`

## Output

Saved clips are written as:

```text
Replay-YYYYMMDD-HHMMSS-<duration>s.mp4
```

By default they go into a `WinInstantReplay` folder under the user's Videos directory when available.

## Temp buffer retention

The app keeps only enough completed segment files to cover the configured replay window, plus a tiny safety margin.

With the default settings:

- replay window: 300 seconds
- segment size: 1 second
- kept temp files: about 302 segments

## Limitations / tradeoffs

- **Windows only**. Non-Windows builds print a stub message.
- **Not fully runtime-tested on Windows from this Linux host.**
  - I was able to run `cargo test` on Linux for cross-platform logic.
  - I also ran `cargo check --target x86_64-pc-windows-gnu` to catch Windows compile issues.
  - I did **not** run the tray icon, global hotkeys, or `gdigrab` capture on a real Windows machine from this environment.
- **Screen only**, no audio by default.
- Replay clips are assembled from the latest completed 1-second segments, so the newest fractional second can be missing if you trigger a hotkey in the middle of a segment.
- This currently records the full desktop using `gdigrab`; it does not yet offer monitor/window selection.
- The app does not currently enforce single-instance behavior.
- Notifications use tray balloons and are intentionally simple.

## Verification done here

On this Linux host I verified:

```bash
cargo test
cargo check --target x86_64-pc-windows-gnu
```

## Suggested Windows smoke test

After building on Windows:

1. make sure `ffmpeg.exe` is on `PATH`
2. start the app
3. confirm the tray icon appears
4. wait 15-20 seconds
5. press `Ctrl+Alt+Shift+1`
6. confirm a replay clip appears in the output folder
7. try the tray menu:
   - Open Output Folder
   - Quit

## Future improvements

If you want to take it further, the next practical upgrades would be:

- optional system audio / microphone capture
- single-instance lock
- selectable monitor
- richer notifications or a tiny settings UI
- cleaner ffmpeg logging to a rotating log file
