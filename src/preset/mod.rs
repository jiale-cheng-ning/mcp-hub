use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct PresetRegistry {
    pub presets: HashMap<String, Preset>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Preset {
    pub description: String,
    pub servers: HashMap<String, PresetServer>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PresetServer {
    #[allow(dead_code)]
    pub package: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
}

/// Load the built-in preset registry.
pub fn load_registry() -> Result<PresetRegistry, String> {
    let yaml = include_str!("../../presets/registry.yml");
    serde_yaml::from_str(yaml).map_err(|e| format!("Failed to parse presets: {}", e))
}

/// Get a preset by name.
pub fn get_preset(name: &str) -> Result<Preset, String> {
    let registry = load_registry()?;
    registry.presets.get(name).cloned().ok_or_else(|| {
        let available: Vec<&str> = registry.presets.keys().map(|s| s.as_str()).collect();
        format!(
            "Unknown preset '{}'. Available: {}",
            name,
            available.join(", ")
        )
    })
}

/// List all available presets.
pub fn list_presets() -> Result<Vec<(String, String)>, String> {
    let registry = load_registry()?;
    let mut presets: Vec<(String, String)> = registry
        .presets
        .iter()
        .map(|(name, p)| (name.clone(), p.description.clone()))
        .collect();
    presets.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(presets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_registry() {
        let registry = load_registry().unwrap();
        assert!(!registry.presets.is_empty());
    }

    #[test]
    fn test_get_preset_exists() {
        let preset = get_preset("minimal").unwrap();
        assert_eq!(
            preset.description,
            "Essential starter pack — filesystem access and GitHub integration"
        );
        assert!(preset.servers.contains_key("filesystem"));
        assert!(preset.servers.contains_key("github"));
    }

    #[test]
    fn test_get_preset_not_found() {
        let result = get_preset("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown preset"));
    }

    #[test]
    fn test_list_presets_sorted() {
        let presets = list_presets().unwrap();
        assert!(presets.len() >= 3);
        // Check sorted
        for i in 1..presets.len() {
            assert!(presets[i - 1].0 <= presets[i].0);
        }
    }

    #[test]
    fn test_webdev_has_playwright() {
        let preset = get_preset("web-dev").unwrap();
        assert!(preset.servers.contains_key("playwright"));
    }

    #[test]
    fn test_fullstack_has_postgres() {
        let preset = get_preset("fullstack").unwrap();
        assert!(preset.servers.contains_key("postgres"));
    }
}
