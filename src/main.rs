#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
mod windows_app;

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = windows_app::run() {
        eprintln!("win-instant-replay failed: {error:#}");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!(
        "win-instant-replay is Windows-only. Build and run it on Windows 10/11 with ffmpeg.exe available."
    );
}
