use std::path::PathBuf;
use std::{fs, io};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub sample_rate_hz: f32,
    pub menu_bar_format: String,
    pub top_n_procs: usize,
    pub history_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sample_rate_hz: 1.0,
            menu_bar_format: "C {cpu} G {gpu} M {mem}".to_string(),
            top_n_procs: 5,
            history_seconds: 120,
        }
    }
}

impl Settings {
    /// Number of samples retained in the ring buffer.
    pub fn history_capacity(&self) -> usize {
        ((self.history_seconds as f32) * self.sample_rate_hz).ceil() as usize
    }

    pub fn config_path() -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("dev", "monitor-rs", "monitor-rs")?;
        Some(dirs.config_dir().join("config.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(self).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let s = Settings::default();
        assert_eq!(s.sample_rate_hz, 1.0);
        assert_eq!(s.history_seconds, 120);
        assert_eq!(s.top_n_procs, 5);
    }

    #[test]
    fn history_capacity_basic() {
        let s = Settings { sample_rate_hz: 1.0, history_seconds: 120, ..Settings::default() };
        assert_eq!(s.history_capacity(), 120);

        let s2 = Settings { sample_rate_hz: 2.0, history_seconds: 60, ..Settings::default() };
        assert_eq!(s2.history_capacity(), 120);
    }

    #[test]
    fn round_trip() {
        let s = Settings::default();
        let j = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&j).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn corrupt_json_falls_back_to_default() {
        let s: Settings = serde_json::from_str("{not valid").unwrap_or_default();
        assert_eq!(s, Settings::default());
    }
}
