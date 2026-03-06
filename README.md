# Win Instant Replay

A small **Windows-only** tray app that keeps a rolling, disk-backed screen recording buffer and saves the last **10 / 30 / 60 / 120 / 300 seconds** on demand with global hotkeys.

It is intentionally simple:

- no installer
- no giant RAM buffer
- just a tray app + `ffmpeg` + replay hotkeys
- optional audio capture when you enable it in config

The core design is still the same: ffmpeg continuously writes short segments to disk, old segments are pruned, and saved clips are assembled from the newest completed files.

> Status: the app logic, tray flow, config model, and Windows-target build path are in place, but real-world validation on multiple Windows machines and audio-device setups is still needed.

## What it supports now

### Capture modes

- **screen only** (default)
- **screen + system audio**
- **screen + microphone**
- **screen + system audio + microphone**

When both audio sources are enabled, they are mixed into a **single AAC audio track** in the saved replay clip.

## Download or build

### Option 1: Download a release zip

When release tags are published, GitHub Actions builds a portable Windows zip that contains:

- `win-instant-replay.exe`
- `README.md`
- `README-WINDOWS.txt`
- `config.example.toml`
- `LICENSE`

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
  - AAC encoding support
  - preferably `wasapi` and/or `dshow` audio input support

You only need the Rust toolchain if you are building from source.

## Installing ffmpeg on Windows

This app depends on **ffmpeg** for both live capture and replay clip assembly.

### Scoop

```powershell
scoop install ffmpeg
```

### Chocolatey

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

You should see `gdigrab` in the input-device list.

For explicit DirectShow audio devices, this is also useful:

```powershell
ffmpeg -hide_banner -list_devices true -f dshow -i dummy
```

That command helps when you need an exact microphone name or a system-audio device such as `Stereo Mix`.

## Quick start

1. Install ffmpeg.
2. Launch `win-instant-replay.exe`.
3. Open **Settings** from the tray icon if you want to change the output directory, replay window, ffmpeg path, segment length, audio settings, or the built-in hotkeys.
4. Wait about **15-20 seconds** so the rolling buffer can fill.
5. Press a hotkey such as `Ctrl+Alt+Shift+1`.
6. Check your output folder for a saved replay clip.

By default the app captures **screen only**. If you want audio, enable it in the Settings window or in `config.toml`.

## How it works

Instead of keeping raw frames in memory, the app uses ffmpeg to continuously write short MP4 segments to disk:

1. the Rust app launches `ffmpeg` with Windows desktop capture (`gdigrab`)
2. optional audio inputs are added if enabled in config
3. ffmpeg writes short MP4 segments into a buffer directory
4. the Rust app deletes old segments beyond the configured replay window
5. when you press a hotkey, the newest completed segments are concatenated into a replay clip

This keeps memory usage low and makes replay saving fast and simple.

## Default behavior

- segment size: **1 second**
- max replay window: **300 seconds**
- frame rate: **30 fps**
- encoder: `libx264`
- preset: `veryfast`
- input: `desktop` via `gdigrab`
- capture mode: **screen only by default**

### Default hotkeys

- `Ctrl+Alt+Shift+1` → save last 10 seconds
- `Ctrl+Alt+Shift+2` → save last 30 seconds
- `Ctrl+Alt+Shift+3` → save last 60 seconds
- `Ctrl+Alt+Shift+4` → save last 120 seconds
- `Ctrl+Alt+Shift+5` → save last 300 seconds

## Tray behavior

The tray icon currently provides:

- **Settings**
- **Open Output Folder**
- **Quit**

Double-clicking the tray icon opens the settings window.

## Config file

On first run, the app creates a default `config.toml` automatically if one does not already exist.

Typical Windows locations are:

- config file: `%APPDATA%\OpenClaw\WinInstantReplay\config\config.toml`
- rolling buffer: `%LOCALAPPDATA%\OpenClaw\WinInstantReplay\cache\buffer`
- saved clips: `%USERPROFILE%\Videos\WinInstantReplay`

See [`config.example.toml`](./config.example.toml).

### Settings window

The tray **Settings** window manages the most common user-facing options:

- output directory
- ffmpeg path
- replay buffer / retention window (`max_replay_seconds`)
- segment length (`segment_seconds`)
- system audio enable/backend/device
- microphone enable/backend/device
- audio sample rate, channel count, and bitrate
- the built-in hotkeys for **10 / 30 / 60 / 120 / 300** second saves

When you click **Save**, the app validates the config, writes `config.toml`, re-registers hotkeys, and restarts the background ffmpeg capture so the new settings take effect immediately.

A couple of practical notes:

- leaving **ffmpeg path** blank means "use `ffmpeg.exe` from `PATH`"
- leaving **output directory** blank means "use the default Videos-based folder"
- audio device boxes are plain text; use exact ffmpeg device names when you are not using defaults like `wasapi + default`

### Main config fields

- `ffmpeg_path` - explicit path to `ffmpeg.exe` if it is not on `PATH`
- `buffer_dir` - optional override for the rolling temp segment directory
- `output_dir` - optional override for saved replay clips
- `max_replay_seconds` - maximum rolling history to keep
- `segment_seconds` - segment size; default is `1`
- `frame_rate` - capture fps
- `encoder` - ffmpeg video encoder, default `libx264`
- `preset` - ffmpeg preset, default `veryfast`
- `ffmpeg_input` - ffmpeg screen input, default `desktop`
- `system_audio_enabled` - enable or disable system audio capture
- `system_audio_backend` - `wasapi` or `dshow`
- `system_audio_device` - audio device string for the system-audio source
- `microphone_enabled` - enable or disable microphone capture
- `microphone_backend` - `wasapi` or `dshow`
- `microphone_device` - audio device string for the microphone source
- `audio_sample_rate` - audio output sample rate, default `48000`
- `audio_channels` - output audio channel count, default `2`
- `audio_bitrate` - AAC bitrate for the output segments, default `192k`
- `ffmpeg_extra_args` - extra args inserted before the segment output options
- `hotkeys` - replay durations and key combos

### Audio configuration notes

The Settings window now exposes the common audio fields directly. Device auto-discovery is still not built in, so the device boxes are plain text fields that expect ffmpeg-friendly device names when you are not using defaults like `wasapi + default`.

#### 1) Screen + system audio

The most practical built-in ffmpeg path on Windows is usually:

```toml
system_audio_enabled = true
system_audio_backend = "wasapi"
system_audio_device = "default"
```

That aims at the current default Windows playback device.

If that does not work on your machine or in your ffmpeg build, try DirectShow instead with an explicit device, for example a `Stereo Mix`-style device if your hardware exposes one:

```toml
system_audio_enabled = true
system_audio_backend = "dshow"
system_audio_device = "Stereo Mix (Realtek(R) Audio)"
```

#### 2) Screen + microphone

Microphone capture is typically configured with an explicit device name:

```toml
microphone_enabled = true
microphone_backend = "dshow"
microphone_device = "Microphone (USB Audio Device)"
```

#### 3) Screen + system audio + microphone

Enable both blocks together. The app mixes them into one audio track in the rolling segments and the saved replay clip.

#### 4) Settings window vs config file

The tray settings window now edits the most common runtime settings, including audio:

- output directory
- ffmpeg path
- replay buffer seconds
- segment length seconds
- system audio enable/backend/device
- microphone enable/backend/device
- audio sample rate, channels, and bitrate
- the built-in **10 / 30 / 60 / 120 / 300** hotkeys

More advanced fields such as `buffer_dir`, `frame_rate`, `encoder`, `preset`, `ffmpeg_input`, and `ffmpeg_extra_args` are still config-file-only.

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

## Example config snippets

Use a fixed ffmpeg path:

```toml
ffmpeg_path = "C:/Tools/ffmpeg/bin/ffmpeg.exe"
```

Save clips somewhere custom:

```toml
output_dir = "D:/Captures/InstantReplay"
```

Enable default-output system audio:

```toml
system_audio_enabled = true
system_audio_backend = "wasapi"
system_audio_device = "default"
```

Enable microphone capture:

```toml
microphone_enabled = true
microphone_backend = "dshow"
microphone_device = "Microphone (USB Audio Device)"
```

Use lighter video encoding:

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

If audio is enabled, the saved replay clip includes it.

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

### Audio was enabled, but the clip is still silent

Things to check:

1. verify the selected audio device really exists
2. for `dshow`, use:

   ```powershell
   ffmpeg -hide_banner -list_devices true -f dshow -i dummy
   ```

3. for system audio, try `wasapi + default` first, then fall back to `dshow` with an explicit device if needed
4. remember that some Windows systems expose no usable `Stereo Mix`-style device in DirectShow
5. if both system audio and microphone are enabled, they are mixed together into one track rather than saved as separate tracks

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

## GPUI note

This repository now includes a practical native Windows settings window, but **not** a GPUI-powered one.

Why: current GPUI documentation explicitly describes GPUI as targeting **macOS and Linux**, not Windows. Because this project is a Windows tray app that needs a real user-facing settings window from the tray, a native Win32 implementation was the most reliable way to satisfy the product goal without pretending the GPUI path was production-ready.

I also did **not** wire in a `gpui-component` / `gpui-components` dependency for the same reason: that stack was not a credible Windows path from this authoring environment.

If GPUI gains solid Windows support later, the settings surface would be a reasonable place to revisit it.

## Limitations and tradeoffs

- **Windows only**. Non-Windows builds print a stub message.
- **The settings window is native Win32, not GPUI.**
  - GPUI currently documents macOS/Linux support rather than Windows support.
  - That made GPUI / gpui-components an impractical choice for a Windows tray app that needs a dependable shipped settings surface today.
- **Not fully runtime-tested on Windows from this Linux authoring environment.**
  - Rust tests were run locally.
  - The updated settings window code was written carefully but not clicked through on a real Windows desktop from here.
  - Real Windows validation of tray behavior, settings save/reload flow, global hotkeys, and multiple audio-device combinations is still needed.
- **System audio capture is practical, not magic.**
  - The default recommendation is `wasapi` with `system_audio_device = "default"`.
  - That is intended to target the active default playback device.
  - If your ffmpeg build or audio stack behaves differently, you may need a DirectShow device like `Stereo Mix`, or you may need to disable system audio.
- **Microphone capture usually needs an explicit device name** when using `dshow`.
- **When both audio sources are enabled, they are mixed into one track**, not stored as separate tracks.
- The app currently records the **full desktop**. There is no monitor/window picker yet.
- The newest fractional second can be missed because clips are assembled from completed segments.
- Single-instance behavior is not fully hardened yet.
- If ffmpeg fails to start in a release build, diagnosis is still rougher than it should be because the app is intentionally quiet.

## Verification completed so far

From this Linux environment:

```bash
~/.cargo/bin/cargo test
~/.cargo/bin/cargo check --target x86_64-pc-windows-gnu
```

## Suggested Windows smoke test

After downloading or building on Windows:

1. make sure `ffmpeg.exe` is installed and reachable
2. launch the app
3. confirm the tray icon appears
4. wait 15-20 seconds
5. press `Ctrl+Alt+Shift+1`
6. confirm a replay clip appears in the output folder
7. open the tray **Settings** window
8. change the output folder, one hotkey, or one audio field, click **Save**, and confirm the app keeps running with the new config
9. enable one audio mode and repeat:
   - system audio only
   - microphone only
   - both together
10. verify the saved clip has the expected audio
11. test tray actions:
   - Settings
   - Open Output Folder
   - Quit
12. edit `config.toml`, relaunch, and confirm custom settings take effect

## CI / release packaging

This repo includes GitHub Actions workflows that:

- build on `windows-latest` for pushes and pull requests
- run tests and a release build
- produce a portable zip artifact
- publish the zip to GitHub Releases when a tag like `v0.1.0` is pushed

## Future improvements

Useful next steps:

- explicit Windows-side audio device discovery in the UI
- browse dialogs / nicer Windows-native pickers for folders and ffmpeg.exe
- per-source volume controls and mute toggles
- better startup error reporting when ffmpeg fails
- single-instance lock hardening
- monitor selection
- richer notifications or a more polished settings UI
- optional auto-start integration
