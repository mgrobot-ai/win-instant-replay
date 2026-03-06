use anyhow::{Context, Result, bail};
use chrono::Local;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::config::AppConfig;
use crate::retention::files_to_delete;

pub struct CaptureSupervisor {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CaptureSupervisor {
    pub fn start(config: Arc<AppConfig>) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let handle = thread::spawn(move || supervise_capture(config, worker_shutdown));
        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CaptureSupervisor {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn supervise_capture(config: Arc<AppConfig>, shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }

        if let Err(error) = fs::create_dir_all(&config.buffer_dir) {
            eprintln!("failed to ensure buffer directory exists: {error:#}");
            thread::sleep(Duration::from_secs(2));
            continue;
        }

        let mut child = match spawn_capture_process(&config) {
            Ok(child) => child,
            Err(error) => {
                eprintln!("failed to start ffmpeg capture: {error:#}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        loop {
            if shutdown.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }

            if let Err(error) = prune_old_segments(&config) {
                eprintln!("segment pruning failed: {error:#}");
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("capture ffmpeg exited with status {status}");
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_secs(1)),
                Err(error) => {
                    eprintln!("failed to poll ffmpeg process: {error:#}");
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
        }

        thread::sleep(Duration::from_secs(2));
    }
}

fn spawn_capture_process(config: &AppConfig) -> Result<Child> {
    let segment_pattern = config
        .buffer_dir
        .join("segment-%Y%m%d-%H%M%S.mp4")
        .to_string_lossy()
        .replace('\\', "/");

    let gop = config
        .frame_rate
        .saturating_mul(config.segment_seconds.max(1));
    let force_keyframes = format!("expr:gte(t,n_forced*{})", config.segment_seconds.max(1));

    let mut command = Command::new(&config.ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-f")
        .arg("gdigrab")
        .arg("-framerate")
        .arg(config.frame_rate.to_string())
        .arg("-draw_mouse")
        .arg("1")
        .arg("-i")
        .arg(&config.ffmpeg_input)
        .arg("-an")
        .arg("-c:v")
        .arg(&config.encoder)
        .arg("-preset")
        .arg(&config.preset)
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-crf")
        .arg("23")
        .arg("-g")
        .arg(gop.to_string())
        .arg("-keyint_min")
        .arg(gop.to_string())
        .arg("-sc_threshold")
        .arg("0")
        .arg("-force_key_frames")
        .arg(force_keyframes);

    for arg in &config.ffmpeg_extra_args {
        command.arg(arg);
    }

    command
        .arg("-f")
        .arg("segment")
        .arg("-segment_time")
        .arg(config.segment_seconds.to_string())
        .arg("-reset_timestamps")
        .arg("1")
        .arg("-strftime")
        .arg("1")
        .arg(segment_pattern)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command.spawn().context("spawning ffmpeg capture process")
}

pub fn save_replay(config: &AppConfig, duration_seconds: u32) -> Result<PathBuf> {
    if duration_seconds == 0 {
        bail!("duration must be greater than zero");
    }

    fs::create_dir_all(&config.output_dir)
        .with_context(|| format!("creating output directory {}", config.output_dir.display()))?;

    // We only use completed segments here. That avoids racing ffmpeg while it is still
    // finalizing the newest chunk, at the cost of up to one segment of recency.
    let segments = collect_complete_segments(&config.buffer_dir, config.segment_seconds)?;
    let needed_segments = duration_seconds.div_ceil(config.segment_seconds.max(1)) as usize;

    if segments.len() < needed_segments {
        bail!(
            "not enough buffered video yet: need {} complete segment(s), have {}",
            needed_segments,
            segments.len()
        );
    }

    let selected = &segments[segments.len() - needed_segments..];
    let list_path = config.buffer_dir.join(format!(
        "concat-{}.txt",
        Local::now().format("%Y%m%d-%H%M%S-%3f")
    ));

    {
        let mut list_file = fs::File::create(&list_path)
            .with_context(|| format!("creating concat list {}", list_path.display()))?;
        for path in selected {
            let normalized = path
                .to_string_lossy()
                .replace('\\', "/")
                .replace('\'', "'\\''");
            writeln!(list_file, "file '{normalized}'")?;
        }
    }

    let output_path = config.output_dir.join(format!(
        "Replay-{}-{}s.mp4",
        Local::now().format("%Y%m%d-%H%M%S"),
        duration_seconds
    ));

    let status = Command::new(&config.ffmpeg_path)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-y")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&list_path)
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
        .arg(&output_path)
        .status()
        .context("running ffmpeg to write replay clip")?;

    let _ = fs::remove_file(&list_path);

    if !status.success() {
        bail!("ffmpeg failed while assembling the replay clip");
    }

    Ok(output_path)
}

pub fn prune_old_segments(config: &AppConfig) -> Result<()> {
    let mut segment_files = Vec::new();
    for entry in fs::read_dir(&config.buffer_dir)
        .with_context(|| format!("reading {}", config.buffer_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if is_segment_file(&path) {
            segment_files.push(path);
        }
    }

    // Retention is count-based because filenames are timestamp sortable and each segment has
    // a fixed duration. That keeps pruning logic simple and avoids tracking extra metadata.
    let keep = config.keep_segment_count();
    let to_delete = files_to_delete(segment_files, keep);
    for path in to_delete {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

fn collect_complete_segments(buffer_dir: &Path, segment_seconds: u32) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let min_age = Duration::from_millis((segment_seconds.max(1) as u64 * 1000) + 250);
    let now = SystemTime::now();

    for entry in
        fs::read_dir(buffer_dir).with_context(|| format!("reading {}", buffer_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !is_segment_file(&path) {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now.duration_since(modified).unwrap_or_default();
        if age < min_age {
            continue;
        }

        files.push(path);
    }

    files.sort();
    Ok(files)
}

fn is_segment_file(path: &Path) -> bool {
    if path.extension().and_then(OsStr::to_str) != Some("mp4") {
        return false;
    }

    path.file_name()
        .and_then(OsStr::to_str)
        .map(|name| name.starts_with("segment-"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::is_segment_file;
    use std::path::Path;

    #[test]
    fn recognizes_segment_names() {
        assert!(is_segment_file(Path::new("segment-20260306-120000.mp4")));
        assert!(!is_segment_file(Path::new("clip.mp4")));
        assert!(!is_segment_file(Path::new("segment-20260306-120000.txt")));
    }
}
