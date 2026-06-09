use std::path::PathBuf;
use std::time::Duration;

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct RawConfig {
    refresh_ms: Option<u64>,
    accent: Option<String>,
}

pub struct Config {
    pub refresh: Duration,
    pub accent: Color,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            refresh: Duration::from_millis(1000),
            accent: Color::Cyan,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let raw: RawConfig = config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();

        let defaults = Config::default();
        Config {
            refresh: raw
                .refresh_ms
                .map(|ms| Duration::from_millis(ms.max(100)))
                .unwrap_or(defaults.refresh),
            accent: raw
                .accent
                .as_deref()
                .and_then(parse_color)
                .unwrap_or(defaults.accent),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("omnitop").join("config.toml"))
}

fn parse_color(name: &str) -> Option<Color> {
    let color = match name.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        other => {
            let hex = other.strip_prefix('#')?;
            if hex.len() != 6 {
                return None;
            }
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    };
    Some(color)
}
