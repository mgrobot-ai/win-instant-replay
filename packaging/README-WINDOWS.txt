Win Instant Replay - Quick Start
================================

This is a portable build. There is no installer yet.

What is included
----------------
- win-instant-replay.exe
- config.example.toml
- README.md
- LICENSE

Before first run
----------------
1. Install ffmpeg on Windows.
2. Make sure ffmpeg.exe is on PATH, or plan to set ffmpeg_path in config.toml.
3. Extract this zip somewhere convenient.

Quick test
----------
1. Launch win-instant-replay.exe.
2. Wait 15-20 seconds.
3. Press Ctrl+Alt+Shift+1.
4. Look for a clip in your Videos\WinInstantReplay folder.

Typical file locations
----------------------
- Config: %APPDATA%\OpenClaw\WinInstantReplay\config\config.toml
- Buffer: %LOCALAPPDATA%\OpenClaw\WinInstantReplay\cache\buffer
- Saved clips: %USERPROFILE%\Videos\WinInstantReplay

If it does not work
-------------------
- Verify ffmpeg runs in PowerShell.
- Set ffmpeg_path explicitly in config.toml if needed.
- Try a different hotkey if another app is already using the default one.
- Read README.md for details, limitations, and troubleshooting.
