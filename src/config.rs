use anyhow::{Context, Result, anyhow, bail};
use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::retention::keep_segment_count;

const DEFAULT_FRAME_RATE: u32 = 30;
const DEFAULT_SEGMENT_SECONDS: u32 = 1;
const DEFAULT_MAX_REPLAY_SECONDS: u32 = 300;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub buffer_dir: PathBuf,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub ffmpeg_path: PathBuf,
    pub buffer_dir: PathBuf,
    pub output_dir: PathBuf,
    pub max_replay_seconds: u32,
    pub segment_seconds: u32,
    pub frame_rate: u32,
    pub encoder: String,
    pub preset: String,
    pub ffmpeg_input: String,
    pub ffmpeg_extra_args: Vec<String>,
    pub hotkeys: Vec<HotkeyBinding>,
}

impl AppConfig {
    pub fn keep_segment_count(&self) -> usize {
        keep_segment_count(self.max_replay_seconds, self.segment_seconds)
    }
}

#[derive(Debug, Clone)]
pub struct HotkeyBinding {
    pub id: i32,
    pub duration_seconds: u32,
    pub combo: String,
    pub parsed: HotKeySpec,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct HotKeySpec {
    pub modifiers: Modifiers,
    pub key: KeyCode,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub struct Modifiers(pub u32);

impl Modifiers {
    pub const ALT: u32 = 0b0001;
    pub const CONTROL: u32 = 0b0010;
    pub const SHIFT: u32 = 0b0100;
    pub const WIN: u32 = 0b1000;

    pub fn contains(self, flag: u32) -> bool {
        self.0 & flag == flag
    }

    fn insert(&mut self, flag: u32) {
        self.0 |= flag;
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum KeyCode {
    Digit(u8),
    Letter(char),
    Function(u8),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FileConfig {
    pub ffmpeg_path: Option<PathBuf>,
    pub buffer_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub max_replay_seconds: u32,
    pub segment_seconds: u32,
    pub frame_rate: u32,
    pub encoder: String,
    pub preset: String,
    pub ffmpeg_input: String,
    pub ffmpeg_extra_args: Vec<String>,
    pub hotkeys: Vec<HotkeyEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HotkeyEntry {
    pub duration_seconds: u32,
    pub combination: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: None,
            buffer_dir: None,
            output_dir: None,
            max_replay_seconds: DEFAULT_MAX_REPLAY_SECONDS,
            segment_seconds: DEFAULT_SEGMENT_SECONDS,
            frame_rate: DEFAULT_FRAME_RATE,
            encoder: "libx264".to_string(),
            preset: "veryfast".to_string(),
            ffmpeg_input: "desktop".to_string(),
            ffmpeg_extra_args: Vec::new(),
            hotkeys: vec![
                HotkeyEntry {
                    duration_seconds: 10,
                    combination: "Ctrl+Alt+Shift+1".to_string(),
                },
                HotkeyEntry {
                    duration_seconds: 30,
                    combination: "Ctrl+Alt+Shift+2".to_string(),
                },
                HotkeyEntry {
                    duration_seconds: 60,
                    combination: "Ctrl+Alt+Shift+3".to_string(),
                },
                HotkeyEntry {
                    duration_seconds: 120,
                    combination: "Ctrl+Alt+Shift+4".to_string(),
                },
                HotkeyEntry {
                    duration_seconds: 300,
                    combination: "Ctrl+Alt+Shift+5".to_string(),
                },
            ],
        }
    }
}

pub fn resolve_paths() -> Result<AppPaths> {
    let project_dirs = ProjectDirs::from("ai", "OpenClaw", "WinInstantReplay")
        .ok_or_else(|| anyhow!("could not determine application directories"))?;

    let config_dir = project_dirs.config_dir().to_path_buf();
    let config_file = config_dir.join("config.toml");
    let buffer_dir = project_dirs.cache_dir().join("buffer");
    let output_dir = UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| project_dirs.data_local_dir().join("clips"))
        .join("WinInstantReplay");

    Ok(AppPaths {
        config_dir,
        config_file,
        buffer_dir,
        output_dir,
    })
}

pub fn load_or_create() -> Result<AppConfig> {
    let paths = resolve_paths()?;
    fs::create_dir_all(&paths.config_dir).with_context(|| {
        format!(
            "creating config directory at {}",
            paths.config_dir.display()
        )
    })?;
    fs::create_dir_all(&paths.buffer_dir).with_context(|| {
        format!(
            "creating buffer directory at {}",
            paths.buffer_dir.display()
        )
    })?;
    fs::create_dir_all(&paths.output_dir).with_context(|| {
        format!(
            "creating output directory at {}",
            paths.output_dir.display()
        )
    })?;

    let file_config = if paths.config_file.exists() {
        let raw = fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        toml::from_str::<FileConfig>(&raw)
            .with_context(|| format!("parsing {}", paths.config_file.display()))?
    } else {
        let default_config = FileConfig::default();
        let rendered = toml::to_string_pretty(&default_config)?;
        fs::write(&paths.config_file, rendered)
            .with_context(|| format!("writing {}", paths.config_file.display()))?;
        default_config
    };

    file_config.into_app_config(&paths)
}

pub fn example_config() -> Result<String> {
    Ok(toml::to_string_pretty(&FileConfig::default())?)
}

impl FileConfig {
    pub fn into_app_config(self, paths: &AppPaths) -> Result<AppConfig> {
        if self.segment_seconds == 0 {
            bail!("segment_seconds must be at least 1");
        }
        if self.frame_rate == 0 {
            bail!("frame_rate must be at least 1");
        }
        if self.max_replay_seconds < 10 {
            bail!("max_replay_seconds must be at least 10");
        }

        let mut seen_durations = HashSet::new();
        let mut seen_hotkeys = HashSet::new();
        let mut hotkeys = Vec::with_capacity(self.hotkeys.len());

        for (index, entry) in self.hotkeys.into_iter().enumerate() {
            if entry.duration_seconds == 0 {
                bail!("hotkey duration must be greater than zero");
            }
            if entry.duration_seconds > self.max_replay_seconds {
                bail!(
                    "hotkey duration {} exceeds max_replay_seconds {}",
                    entry.duration_seconds,
                    self.max_replay_seconds
                );
            }
            if !seen_durations.insert(entry.duration_seconds) {
                bail!("duplicate hotkey duration {}", entry.duration_seconds);
            }

            let parsed = HotKeySpec::parse(&entry.combination)
                .with_context(|| format!("parsing hotkey '{}'", entry.combination))?;
            if !seen_hotkeys.insert(parsed) {
                bail!("duplicate hotkey combination '{}'", entry.combination);
            }

            hotkeys.push(HotkeyBinding {
                id: (index + 1) as i32,
                duration_seconds: entry.duration_seconds,
                combo: entry.combination,
                parsed,
            });
        }

        hotkeys.sort_by_key(|hotkey| hotkey.duration_seconds);

        Ok(AppConfig {
            ffmpeg_path: self
                .ffmpeg_path
                .unwrap_or_else(|| PathBuf::from("ffmpeg.exe")),
            buffer_dir: self.buffer_dir.unwrap_or_else(|| paths.buffer_dir.clone()),
            output_dir: self.output_dir.unwrap_or_else(|| paths.output_dir.clone()),
            max_replay_seconds: self.max_replay_seconds,
            segment_seconds: self.segment_seconds,
            frame_rate: self.frame_rate,
            encoder: self.encoder,
            preset: self.preset,
            ffmpeg_input: self.ffmpeg_input,
            ffmpeg_extra_args: self.ffmpeg_extra_args,
            hotkeys,
        })
    }
}

impl HotKeySpec {
    pub fn parse(input: &str) -> Result<Self> {
        let tokens: Vec<_> = input
            .split('+')
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .collect();

        if tokens.len() < 2 {
            bail!("hotkey must include at least one modifier and one key");
        }

        let mut modifiers = Modifiers::default();
        let mut key = None;

        for token in tokens {
            match token.to_ascii_lowercase().as_str() {
                "alt" => modifiers.insert(Modifiers::ALT),
                "ctrl" | "control" => modifiers.insert(Modifiers::CONTROL),
                "shift" => modifiers.insert(Modifiers::SHIFT),
                "win" | "meta" | "super" => modifiers.insert(Modifiers::WIN),
                other => {
                    if key.is_some() {
                        bail!("hotkey may only contain one non-modifier key");
                    }
                    key = Some(parse_key(other)?);
                }
            }
        }

        if modifiers.is_empty() {
            bail!("hotkey must include at least one modifier");
        }

        let key = key.ok_or_else(|| anyhow!("hotkey is missing a key"))?;
        Ok(Self { modifiers, key })
    }
}

fn parse_key(token: &str) -> Result<KeyCode> {
    let token = token.trim();
    if token.len() == 1 {
        let ch = token.chars().next().unwrap();
        if ch.is_ascii_digit() {
            return Ok(KeyCode::Digit(ch as u8 - b'0'));
        }
        if ch.is_ascii_alphabetic() {
            return Ok(KeyCode::Letter(ch.to_ascii_uppercase()));
        }
    }

    if let Some(rest) = token.strip_prefix('f').or_else(|| token.strip_prefix('F')) {
        let number = rest
            .parse::<u8>()
            .with_context(|| format!("unsupported key '{token}'"))?;
        if (1..=24).contains(&number) {
            return Ok(KeyCode::Function(number));
        }
    }

    bail!("unsupported key '{token}'")
}

#[cfg(test)]
mod tests {
    use super::{AppPaths, FileConfig, HotKeySpec, KeyCode, Modifiers};
    use std::path::PathBuf;

    fn fake_paths() -> AppPaths {
        AppPaths {
            config_dir: PathBuf::from("config"),
            config_file: PathBuf::from("config/config.toml"),
            buffer_dir: PathBuf::from("buffer"),
            output_dir: PathBuf::from("output"),
        }
    }

    #[test]
    fn parses_hotkey_combo() {
        let parsed = HotKeySpec::parse("Ctrl+Alt+Shift+4").unwrap();
        assert!(parsed.modifiers.contains(Modifiers::CONTROL));
        assert!(parsed.modifiers.contains(Modifiers::ALT));
        assert!(parsed.modifiers.contains(Modifiers::SHIFT));
        assert_eq!(parsed.key, KeyCode::Digit(4));
    }

    #[test]
    fn rejects_modifier_only_hotkeys() {
        let error = HotKeySpec::parse("Ctrl+Alt").unwrap_err().to_string();
        assert!(error.contains("missing a key") || error.contains("one modifier"));
    }

    #[test]
    fn validates_hotkeys_against_max_replay() {
        let mut config = FileConfig::default();
        config.max_replay_seconds = 60;
        let error = config
            .into_app_config(&fake_paths())
            .unwrap_err()
            .to_string();
        assert!(error.contains("exceeds max_replay_seconds"));
    }

    #[test]
    fn resolves_default_paths_when_not_overridden() {
        let mut config = FileConfig::default();
        config.max_replay_seconds = 300;
        let app = config.into_app_config(&fake_paths()).unwrap();
        assert_eq!(app.buffer_dir, PathBuf::from("buffer"));
        assert_eq!(app.output_dir, PathBuf::from("output"));
    }
}
