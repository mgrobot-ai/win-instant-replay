# Win Instant Replay

A small **Windows-only** tray app that keeps a rolling, disk-backed screen recording buffer and saves the last **10 / 30 / 60 / 120 / 300 seconds** on demand with global hotkeys.

It is intentionally simple:

- no installer
- no heavyweight UI
- no giant RAM buffer
- just a tray app + `ffmpeg` + replay hotkeys

> Status: the app logic and Windows build path are in place, but the tray behavior, hotkeys, and live desktop capture still need real-world testing on Windows hardware.

## Download or build

### Option 1: Download a release zip

When release tags are published, GitHub Actions builds a portable Windows zip that contains:

- `win-instant-replay.exe`
- `README.md`
- `README-WINDOWS.txt`
- `config.example.toml`
- `LICENSE`

Release zips are meant to be the easiest way to try the app.

### Option 2: Build from source

From a Windows PowerShell prompt in this project directory:

```powershell
cargo build --release
```

The binary will be created at:

```text
target\release\win-instant-replay.exe
```

Release builds use the Windows subsystem flag, so the app should not open a console window.

## Requirements

- Windows 10 or 11
- `ffmpeg.exe` available on `PATH`, or configured explicitly in `config.toml`
- an ffmpeg build with:
  - `gdigrab`
  - `libx264`

You only need the Rust toolchain if you are building from source.

## Installing ffmpeg on Windows

This app depends on **ffmpeg** for both live capture and replay clip assembly.

### Easiest options

#### Scoop

```powershell
scoop install ffmpeg
```

#### Chocolatey

```powershell
choco install ffmpeg
```

### Manual install

1. Download a Windows ffmpeg build.
2. Extract it somewhere like `C:\Tools\ffmpeg`.
3. Make sure `ffmpeg.exe` exists under a path such as:

   ```text
   C:\Tools\ffmpeg\bin\ffmpeg.exe
   ```

4. Either:
   - add that `bin` folder to your Windows `PATH`, or
   - set `ffmpeg_path` in the app config file.

### Quick verification

In PowerShell:

```powershell
ffmpeg -hide_banner -devices
```

You should be able to run `ffmpeg` successfully, and the output should include support for Windows desktop capture via `gdigrab`.

## Quick start

1. Install ffmpeg.
2. Launch `win-instant-replay.exe`.
3. Wait about **15-20 seconds** so the rolling buffer can fill.
4. Press a hotkey such as `Ctrl+Alt+Shift+1`.
5. Check your output folder for a saved replay clip.

If the tray icon appears but clips never save, the most common cause is ffmpeg not being installed correctly or not being reachable by the app.

## How it works

Instead of keeping raw video frames in memory, the app uses ffmpeg to continuously write short MP4 segments to disk:

1. the Rust app launches `ffmpeg` with Windows desktop capture (`gdigrab`)
2. ffmpeg writes 1-second MP4 segments into a temp buffer directory
3. the Rust app deletes old segments beyond the configured replay window
4. when you press a hotkey, the newest completed segments are concatenated into a replay clip

This keeps memory usage low and makes replay saving fast and simple.

## Default behavior

- segment size: **1 second**
- max replay window: **300 seconds**
- frame rate: **30 fps**
- encoder: `libx264`
- preset: `veryfast`
- input: `desktop` via `gdigrab`
- capture mode: **screen only** (no audio by default)

### Default hotkeys

- `Ctrl+Alt+Shift+1` → save last 10 seconds
- `Ctrl+Alt+Shift+2` → save last 30 seconds
- `Ctrl+Alt+Shift+3` → save last 60 seconds
- `Ctrl+Alt+Shift+4` → save last 120 seconds
- `Ctrl+Alt+Shift+5` → save last 300 seconds

## Tray behavior

The tray icon currently provides:

- **Open Output Folder**
- **Quit**

Double-clicking the tray icon also opens the output folder.

## Config file

On first run, the app creates a default `config.toml` automatically if one does not already exist.

Typical Windows locations are:

- config file: `%APPDATA%\OpenClaw\WinInstantReplay\config\config.toml`
- rolling buffer: `%LOCALAPPDATA%\OpenClaw\WinInstantReplay\cache\buffer`
- saved clips: `%USERPROFILE%\Videos\WinInstantReplay`

> Exact paths can vary a little by Windows setup, but those are the intended defaults.

See [`config.example.toml`](./config.example.toml).

### Main config fields

- `ffmpeg_path` - explicit path to `ffmpeg.exe` if it is not on `PATH`
- `buffer_dir` - optional override for the rolling temp segment directory
- `output_dir` - optional override for saved replay clips
- `max_replay_seconds` - maximum rolling history to keep
- `segment_seconds` - segment size; default is `1`
- `frame_rate` - capture fps
- `encoder` - ffmpeg video encoder, default `libx264`
- `preset` - ffmpeg preset, default `veryfast`
- `ffmpeg_input` - ffmpeg capture input, default `desktop`
- `ffmpeg_extra_args` - extra args inserted before the segment output options
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

### Example config tweaks

Use a fixed ffmpeg path:

```toml
ffmpeg_path = "C:/Tools/ffmpeg/bin/ffmpeg.exe"
```

Save clips somewhere custom:

```toml
output_dir = "D:/Captures/InstantReplay"
```

Use slightly lighter encoding settings:

```toml
preset = "superfast"
```

## Output naming

Saved clips are written like this:

```text
Replay-YYYYMMDD-HHMMSS-<duration>s.mp4
```

Example:

```text
Replay-20260306-081530-30s.mp4
```

## Temp buffer retention

The app keeps only enough completed segment files to cover the configured replay window, plus a small safety margin.

With the default settings:

- replay window: 300 seconds
- segment size: 1 second
- kept temp files: about 302 segments

## Troubleshooting

### The tray icon appears, but no replay files are saved

Check these first:

1. confirm `ffmpeg.exe` is installed
2. confirm `ffmpeg` runs from PowerShell
3. if needed, set `ffmpeg_path` explicitly in `config.toml`
4. wait at least 10-20 seconds before testing a hotkey
5. make sure the chosen hotkey is not already claimed by another app

### Hotkeys do nothing

Possible causes:

- another app already uses the same global hotkey
- the app has not buffered enough video yet
- ffmpeg never started successfully

Try changing the hotkeys in `config.toml` to something less likely to conflict, such as `Ctrl+Alt+F9`.

### Replays are missing the newest split-second

That is expected with the current design. Replays are assembled from the newest **completed** segments, so the most recent partial segment can be omitted.

### ffmpeg is installed, but the app still cannot find it

Set an explicit path in `config.toml`, for example:

```toml
ffmpeg_path = "C:/Tools/ffmpeg/bin/ffmpeg.exe"
```

### Where did my config file go?

The config file is **not** stored next to the `.exe`.
It lives in the app config directory under your Windows profile, typically:

```text
%APPDATA%\OpenClaw\WinInstantReplay\config\config.toml
```

## Limitations and current tradeoffs

- **Windows only**. Non-Windows builds print a stub message.
- **Not fully runtime-tested on Windows from this Linux authoring environment.**
  - Cross-platform Rust tests were run.
  - Windows-target compilation was checked.
  - Real Windows validation of the tray icon, hotkeys, and `gdigrab` capture is still needed.
- **Screen only** by default. Audio capture is not enabled yet.
- The app currently records the **full desktop**. There is no monitor/window picker yet.
- The newest fractional second can be missed because clips are assembled from completed segments.
- Single-instance behavior is not enforced yet.
- If ffmpeg fails to start in a release build, diagnosis is still rougher than it should be because the app is intentionally quiet.

## Verification completed so far

From this Linux environment:

```bash
cargo test
cargo check --target x86_64-pc-windows-gnu
```

GitHub Actions workflows are also included to build and package Windows artifacts automatically on push / pull request, and to publish release zips on version tags.

## Suggested Windows smoke test

After downloading or building on Windows:

1. make sure `ffmpeg.exe` is installed and reachable
2. launch the app
3. confirm the tray icon appears
4. wait 15-20 seconds
5. press `Ctrl+Alt+Shift+1`
6. confirm a replay clip appears in the output folder
7. test tray actions:
   - Open Output Folder
   - Quit
8. edit `config.toml`, relaunch, and confirm custom settings take effect

## CI / release packaging

This repo includes GitHub Actions workflows that:

- build on `windows-latest` for pushes and pull requests
- run tests and a release build
- produce a portable zip artifact
- publish the zip to GitHub Releases when a tag like `v0.1.0` is pushed

The release zip is intentionally simple and portable. It does **not** try to install ffmpeg or register the app automatically.

## Future improvements

Practical next steps, if this project keeps growing:

- optional system audio / microphone capture
- better startup error reporting when ffmpeg fails
- single-instance lock
- monitor selection
- richer notifications or a tiny settings UI
- optional auto-start integration
