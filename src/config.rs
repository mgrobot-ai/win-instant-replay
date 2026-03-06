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
const DEFAULT_AUDIO_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_AUDIO_CHANNELS: u32 = 2;
const DEFAULT_AUDIO_BITRATE: &str = "192k";
const DEFAULT_FFMPEG_PATH: &str = "ffmpeg.exe";
pub const SETTINGS_HOTKEY_DURATIONS: [u32; 5] = [10, 30, 60, 120, 300];
const DEFAULT_HOTKEYS: &[(u32, &str)] = &[
    (10, "Ctrl+Alt+Shift+1"),
    (30, "Ctrl+Alt+Shift+2"),
    (60, "Ctrl+Alt+Shift+3"),
    (120, "Ctrl+Alt+Shift+4"),
    (300, "Ctrl+Alt+Shift+5"),
];

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
    pub system_audio_enabled: bool,
    pub system_audio_backend: AudioBackend,
    pub system_audio_device: String,
    pub microphone_enabled: bool,
    pub microphone_backend: AudioBackend,
    pub microphone_device: String,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
    pub audio_bitrate: String,
    pub ffmpeg_extra_args: Vec<String>,
    pub hotkeys: Vec<HotkeyBinding>,
}

impl AppConfig {
    pub fn keep_segment_count(&self) -> usize {
        keep_segment_count(self.max_replay_seconds, self.segment_seconds)
    }

    pub fn hotkey_combination(&self, duration_seconds: u32) -> Option<&str> {
        self.hotkeys
            .iter()
            .find(|binding| binding.duration_seconds == duration_seconds)
            .map(|binding| binding.combo.as_str())
    }

    pub fn to_file_config(&self, paths: &AppPaths) -> FileConfig {
        FileConfig {
            ffmpeg_path: if self.ffmpeg_path == PathBuf::from(DEFAULT_FFMPEG_PATH) {
                None
            } else {
                Some(self.ffmpeg_path.clone())
            },
            buffer_dir: if self.buffer_dir == paths.buffer_dir {
                None
            } else {
                Some(self.buffer_dir.clone())
            },
            output_dir: if self.output_dir == paths.output_dir {
                None
            } else {
                Some(self.output_dir.clone())
            },
            max_replay_seconds: self.max_replay_seconds,
            segment_seconds: self.segment_seconds,
            frame_rate: self.frame_rate,
            encoder: self.encoder.clone(),
            preset: self.preset.clone(),
            ffmpeg_input: self.ffmpeg_input.clone(),
            system_audio_enabled: self.system_audio_enabled,
            system_audio_backend: self.system_audio_backend.as_str().to_string(),
            system_audio_device: self.system_audio_device.clone(),
            microphone_enabled: self.microphone_enabled,
            microphone_backend: self.microphone_backend.as_str().to_string(),
            microphone_device: self.microphone_device.clone(),
            audio_sample_rate: self.audio_sample_rate,
            audio_channels: self.audio_channels,
            audio_bitrate: self.audio_bitrate.clone(),
            ffmpeg_extra_args: self.ffmpeg_extra_args.clone(),
            hotkeys: self
                .hotkeys
                .iter()
                .map(|binding| HotkeyEntry {
                    duration_seconds: binding.duration_seconds,
                    combination: binding.combo.clone(),
                })
                .collect(),
        }
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AudioBackend {
    Wasapi,
    Dshow,
}

impl AudioBackend {
    pub fn parse(value: &str, field_name: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "wasapi" => Ok(Self::Wasapi),
            "dshow" => Ok(Self::Dshow),
            _ => bail!("{field_name} must be either 'wasapi' or 'dshow'"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wasapi => "wasapi",
            Self::Dshow => "dshow",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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
    pub system_audio_enabled: bool,
    pub system_audio_backend: String,
    pub system_audio_device: String,
    pub microphone_enabled: bool,
    pub microphone_backend: String,
    pub microphone_device: String,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
    pub audio_bitrate: String,
    pub ffmpeg_extra_args: Vec<String>,
    pub hotkeys: Vec<HotkeyEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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
            system_audio_enabled: false,
            system_audio_backend: "wasapi".to_string(),
            system_audio_device: "default".to_string(),
            microphone_enabled: false,
            microphone_backend: "dshow".to_string(),
            microphone_device: String::new(),
            audio_sample_rate: DEFAULT_AUDIO_SAMPLE_RATE,
            audio_channels: DEFAULT_AUDIO_CHANNELS,
            audio_bitrate: DEFAULT_AUDIO_BITRATE.to_string(),
            ffmpeg_extra_args: Vec::new(),
            hotkeys: default_hotkey_entries(),
        }
    }
}

pub fn default_hotkey_combination(duration_seconds: u32) -> Option<&'static str> {
    DEFAULT_HOTKEYS
        .iter()
        .find(|(duration, _)| *duration == duration_seconds)
        .map(|(_, combination)| *combination)
}

pub fn default_hotkey_entries() -> Vec<HotkeyEntry> {
    DEFAULT_HOTKEYS
        .iter()
        .map(|(duration_seconds, combination)| HotkeyEntry {
            duration_seconds: *duration_seconds,
            combination: (*combination).to_string(),
        })
        .collect()
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
    let (_, config) = load_or_create_with_paths()?;
    Ok(config)
}

pub fn load_or_create_with_paths() -> Result<(AppPaths, AppConfig)> {
    let paths = resolve_paths()?;
    let file_config = load_or_create_file_config(&paths)?;
    let config = file_config.into_app_config(&paths)?;
    ensure_runtime_dirs(&config, &paths)?;
    Ok((paths, config))
}

pub fn load_or_create_file_config(paths: &AppPaths) -> Result<FileConfig> {
    fs::create_dir_all(&paths.config_dir).with_context(|| {
        format!(
            "creating config directory at {}",
            paths.config_dir.display()
        )
    })?;

    let file_config = if paths.config_file.exists() {
        let raw = fs::read_to_string(&paths.config_file)
            .with_context(|| format!("reading {}", paths.config_file.display()))?;
        toml::from_str::<FileConfig>(&raw)
            .with_context(|| format!("parsing {}", paths.config_file.display()))?
    } else {
        let default_config = FileConfig::default();
        save_file_config(paths, &default_config)?;
        default_config
    };

    Ok(file_config)
}

pub fn save_file_config(paths: &AppPaths, config: &FileConfig) -> Result<()> {
    fs::create_dir_all(&paths.config_dir).with_context(|| {
        format!(
            "creating config directory at {}",
            paths.config_dir.display()
        )
    })?;

    let rendered = toml::to_string_pretty(config)?;
    fs::write(&paths.config_file, rendered)
        .with_context(|| format!("writing {}", paths.config_file.display()))?;

    Ok(())
}

pub fn ensure_runtime_dirs(config: &AppConfig, paths: &AppPaths) -> Result<()> {
    fs::create_dir_all(&paths.config_dir).with_context(|| {
        format!(
            "creating config directory at {}",
            paths.config_dir.display()
        )
    })?;
    fs::create_dir_all(&config.buffer_dir).with_context(|| {
        format!(
            "creating buffer directory at {}",
            config.buffer_dir.display()
        )
    })?;
    fs::create_dir_all(&config.output_dir).with_context(|| {
        format!(
            "creating output directory at {}",
            config.output_dir.display()
        )
    })?;
    Ok(())
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
        if self.audio_sample_rate == 0 {
            bail!("audio_sample_rate must be at least 1");
        }
        if self.audio_channels == 0 {
            bail!("audio_channels must be at least 1");
        }
        if self.audio_bitrate.trim().is_empty() {
            bail!("audio_bitrate must not be empty");
        }

        let system_audio_backend =
            AudioBackend::parse(&self.system_audio_backend, "system_audio_backend")?;
        let microphone_backend =
            AudioBackend::parse(&self.microphone_backend, "microphone_backend")?;

        if self.system_audio_enabled && self.system_audio_device.trim().is_empty() {
            bail!("system_audio_device must not be empty when system_audio_enabled is true");
        }
        if self.microphone_enabled && self.microphone_device.trim().is_empty() {
            bail!("microphone_device must not be empty when microphone_enabled is true");
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
                .unwrap_or_else(|| PathBuf::from(DEFAULT_FFMPEG_PATH)),
            buffer_dir: self.buffer_dir.unwrap_or_else(|| paths.buffer_dir.clone()),
            output_dir: self.output_dir.unwrap_or_else(|| paths.output_dir.clone()),
            max_replay_seconds: self.max_replay_seconds,
            segment_seconds: self.segment_seconds,
            frame_rate: self.frame_rate,
            encoder: self.encoder,
            preset: self.preset,
            ffmpeg_input: self.ffmpeg_input,
            system_audio_enabled: self.system_audio_enabled,
            system_audio_backend,
            system_audio_device: self.system_audio_device,
            microphone_enabled: self.microphone_enabled,
            microphone_backend,
            microphone_device: self.microphone_device,
            audio_sample_rate: self.audio_sample_rate,
            audio_channels: self.audio_channels,
            audio_bitrate: self.audio_bitrate,
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
    use super::{
        AppConfig, AppPaths, AudioBackend, FileConfig, HotKeySpec, KeyCode, Modifiers,
        default_hotkey_combination,
    };
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
        assert!(!app.system_audio_enabled);
        assert!(!app.microphone_enabled);
        assert_eq!(app.system_audio_backend, AudioBackend::Wasapi);
        assert_eq!(app.microphone_backend, AudioBackend::Dshow);
    }

    #[test]
    fn rejects_invalid_audio_backend() {
        let mut config = FileConfig::default();
        config.system_audio_backend = "magic".to_string();
        let error = config
            .into_app_config(&fake_paths())
            .unwrap_err()
            .to_string();
        assert!(error.contains("system_audio_backend"));
    }

    #[test]
    fn rejects_enabled_microphone_without_device_name() {
        let mut config = FileConfig::default();
        config.microphone_enabled = true;
        let error = config
            .into_app_config(&fake_paths())
            .unwrap_err()
            .to_string();
        assert!(error.contains("microphone_device"));
    }

    #[test]
    fn serializes_and_deserializes_default_file_config() {
        let config = FileConfig::default();
        let rendered = toml::to_string_pretty(&config).unwrap();
        let reparsed: FileConfig = toml::from_str(&rendered).unwrap();
        assert_eq!(reparsed, config);
    }

    #[test]
    fn converts_runtime_config_back_to_sparse_file_config() {
        let app = AppConfig {
            ffmpeg_path: PathBuf::from("ffmpeg.exe"),
            buffer_dir: PathBuf::from("buffer"),
            output_dir: PathBuf::from("output"),
            max_replay_seconds: 300,
            segment_seconds: 1,
            frame_rate: 30,
            encoder: "libx264".to_string(),
            preset: "veryfast".to_string(),
            ffmpeg_input: "desktop".to_string(),
            system_audio_enabled: false,
            system_audio_backend: AudioBackend::Wasapi,
            system_audio_device: "default".to_string(),
            microphone_enabled: false,
            microphone_backend: AudioBackend::Dshow,
            microphone_device: String::new(),
            audio_sample_rate: 48_000,
            audio_channels: 2,
            audio_bitrate: "192k".to_string(),
            ffmpeg_extra_args: Vec::new(),
            hotkeys: FileConfig::default()
                .into_app_config(&fake_paths())
                .unwrap()
                .hotkeys,
        };

        let file = app.to_file_config(&fake_paths());
        assert_eq!(file.ffmpeg_path, None);
        assert_eq!(file.buffer_dir, None);
        assert_eq!(file.output_dir, None);
        assert_eq!(file.system_audio_backend, "wasapi");
        assert_eq!(file.microphone_backend, "dshow");
    }

    #[test]
    fn provides_default_hotkeys_for_managed_durations() {
        assert_eq!(default_hotkey_combination(120), Some("Ctrl+Alt+Shift+4"));
        assert_eq!(default_hotkey_combination(999), None);
    }
}
