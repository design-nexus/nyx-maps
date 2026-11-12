use nyx_core::paths;
pub use nyx_theme::{ThemeMode, ThemePrefs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub static PREFS: LazyLock<Mutex<Settings>> = LazyLock::new(|| Mutex::new(Settings::load()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub mode: ThemeMode,
    pub theme: String,
    pub glow: bool,
    pub reduce_motion: bool,

    #[serde(default)]
    pub geoapify_api_key: String,

    #[serde(default = "default_style")]
    pub map_style: String,

    #[serde(default = "default_travel_mode")]
    pub travel_mode: String,

    #[serde(default)]
    pub custom_satellite_url: String,

    #[serde(default)]
    pub custom_topo_url: String,

    #[serde(default = "default_units")]
    pub units: String, // "imperial" or "metric"

    #[serde(default = "default_lat")]
    pub last_lat: f64,

    #[serde(default = "default_lon")]
    pub last_lon: f64,

    #[serde(default = "default_zoom")]
    pub last_zoom: f64,

    #[serde(default)]
    pub last_current_lat: Option<f64>,
    #[serde(default)]
    pub last_current_lon: Option<f64>,
    #[serde(default)]
    pub last_current_label: Option<String>,

    #[serde(default)]
    pub home_address: String,
    #[serde(default)]
    pub home_lat: Option<f64>,
    #[serde(default)]
    pub home_lon: Option<f64>,

    #[serde(default)]
    pub work_address: String,
    #[serde(default)]
    pub work_lat: Option<f64>,
    #[serde(default)]
    pub work_lon: Option<f64>,

    /// When the desktop's location service can't answer, guess from the public IP
    /// (ipwho.is, then ipapi.co). Off: the IP never leaves for that.
    #[serde(default)]
    pub ip_location: bool,
}

fn default_travel_mode() -> String { "drive".into() }

fn default_style() -> String {
    "standard".to_string()
}

fn default_units() -> String {
    "imperial".to_string()
}

fn default_lat() -> f64 {
    36.1627 // Nashville
}

fn default_lon() -> f64 {
    -86.7816
}

fn default_zoom() -> f64 {
    12.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".to_string(),
            glow: true,
            reduce_motion: false,
            geoapify_api_key: String::new(),
            map_style: default_style(),
            travel_mode: default_travel_mode(),
            custom_satellite_url: String::new(),
            custom_topo_url: String::new(),
            units: default_units(),
            last_lat: default_lat(),
            last_lon: default_lon(),
            last_zoom: default_zoom(),
            last_current_lat: None,
            last_current_lon: None,
            last_current_label: None,
            home_address: String::new(),
            home_lat: None,
            home_lon: None,
            work_address: String::new(),
            work_lat: None,
            work_lon: None,
            ip_location: false,
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        paths::config_home().join("nyx-maps").join("settings.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(settings) = toml::from_str::<Settings>(&content) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write(&path, content);
        }
    }

    #[allow(dead_code)]
    pub fn theme_prefs(&self) -> ThemePrefs {
        ThemePrefs {
            mode: self.mode.clone(),
            theme: self.theme.clone(),
            glow: self.glow,
            reduce_motion: self.reduce_motion,
        }
    }
}

pub fn get() -> Settings {
    PREFS.lock().unwrap().clone()
}

pub fn update<F: FnOnce(&mut Settings)>(f: F) {
    let mut prefs = PREFS.lock().unwrap();
    f(&mut prefs);
    prefs.save();
}
