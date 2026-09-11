//! Persisted user settings: interface language, download options, and UI
//! state, stored as JSON next to the tool cache:
//!
//!   %LOCALAPPDATA%\CatchYT\settings.json
//!
//! Loading is deliberately forgiving — a missing, corrupt, or older-version
//! file must never block startup, so any problem falls back to defaults
//! (unknown fields are ignored, missing fields take their default value).

use crate::engine::options::{default_music_dir, DownloadOptions};
use crate::i18n::Lang;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: Lang,
    pub opts: DownloadOptions,
    pub show_logs: bool,
}

/// Where the settings file lives.
pub fn settings_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("cannot resolve local app data directory"))?;
    Ok(base.data_local_dir().join("CatchYT").join("settings.json"))
}

impl Settings {
    /// Load from the default location, falling back to defaults on any error.
    pub fn load() -> Settings {
        settings_path()
            .map(|path| Settings::load_from(&path))
            .unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Settings {
        let mut settings: Settings = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // The saved output folder may have been renamed, deleted, or live on a
        // drive that is no longer plugged in.
        if !settings.opts.output_dir.is_dir() {
            settings.opts.output_dir = default_music_dir();
        }
        settings
    }

    /// Save to the default location.
    pub fn save(&self) -> Result<()> {
        self.save_to(&settings_path()?)
    }

    /// Write through a staging file so an interrupted save can never leave a
    /// truncated settings file behind.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self).context("serializing settings")?;
        let staging = path.with_extension("json.part");
        let result = (|| -> Result<()> {
            fs::write(&staging, &json).with_context(|| format!("writing {}", staging.display()))?;
            if path.exists() {
                fs::remove_file(path)
                    .with_context(|| format!("removing stale {}", path.display()))?;
            }
            fs::rename(&staging, path).with_context(|| format!("publishing {}", path.display()))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staging);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::options::NamingPreset;
    use crate::engine::AudioFormat;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let unique = format!(
                "catchyt-settings-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn settings_round_trip() {
        let dir = TestDir::new();
        let path = dir.0.join("settings.json");

        let mut settings = Settings {
            lang: Lang::En,
            show_logs: true,
            ..Settings::default()
        };
        settings.opts.audio_format = AudioFormat::Flac;
        settings.opts.naming = NamingPreset::Custom("%(id)s.%(ext)s".to_string());
        settings.opts.output_dir = dir.0.clone(); // exists, so it survives load

        settings.save_to(&path).unwrap();
        let loaded = Settings::load_from(&path);

        assert_eq!(loaded.lang, Lang::En);
        assert!(loaded.show_logs);
        assert_eq!(loaded.opts.audio_format, AudioFormat::Flac);
        assert_eq!(
            loaded.opts.naming,
            NamingPreset::Custom("%(id)s.%(ext)s".to_string())
        );
        assert_eq!(loaded.opts.output_dir, dir.0);
        assert!(!path.with_extension("json.part").exists());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = TestDir::new();
        let loaded = Settings::load_from(&dir.0.join("nope.json"));
        assert_eq!(loaded.lang, Lang::Fr);
        assert!(!loaded.show_logs);
    }

    #[test]
    fn corrupt_file_yields_defaults() {
        let dir = TestDir::new();
        let path = dir.0.join("settings.json");
        fs::write(&path, b"{ this is not json").unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.lang, Lang::Fr);
    }

    #[test]
    fn partial_file_fills_missing_fields_with_defaults() {
        // A settings file from an older (or newer) version of the app.
        let dir = TestDir::new();
        let path = dir.0.join("settings.json");
        fs::write(&path, br#"{ "lang": "En", "unknown_future_field": 42 }"#).unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.lang, Lang::En);
        assert!(!loaded.show_logs);
    }

    #[test]
    fn vanished_output_dir_falls_back_to_music_dir() {
        let dir = TestDir::new();
        let path = dir.0.join("settings.json");

        let mut settings = Settings::default();
        settings.opts.output_dir = dir.0.join("no-longer-here");
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.opts.output_dir, default_music_dir());
    }

    #[test]
    fn save_replaces_an_existing_file() {
        let dir = TestDir::new();
        let path = dir.0.join("settings.json");

        Settings::default().save_to(&path).unwrap();
        let updated = Settings {
            lang: Lang::En,
            ..Settings::default()
        };
        updated.save_to(&path).unwrap();

        assert_eq!(Settings::load_from(&path).lang, Lang::En);
    }
}
