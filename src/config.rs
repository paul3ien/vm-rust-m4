use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VMConfig {
    pub name: String,
    pub cpu_count: u32,
    pub memory_size_gb: u32,
    pub disk_path: Option<String>,
    pub disk_size_gb: u32,
    /// ISO d'installation (mode installation).
    pub iso_path: Option<String>,
    /// ISO cloud-init seed (cidata.iso) pour image cloud pre-installee.
    pub seed_path: Option<String>,
}

impl Default for VMConfig {
    fn default() -> Self {
        Self {
            name: "UbuntuEFI".into(),
            cpu_count: 4,
            memory_size_gb: 4,
            disk_path: Some("data/ubuntu-cloud-raw.img".into()),
            disk_size_gb: 10,
            iso_path: None,
            seed_path: Some("data/cidata.iso".into()),
        }
    }
}

impl VMConfig {
    pub fn load_or_default(path: &str) -> Self {
        if Path::new(path).exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str::<VMConfig>(&content) {
                    Ok(config) => {
                        log::info!("Configuration chargee depuis {}", path);
                        return config;
                    }
                    Err(e) => {
                        log::warn!(
                            "Erreur de parsing dans {}: {}, utilisation des defauts",
                            path,
                            e
                        );
                    }
                },
                Err(e) => {
                    log::warn!("Impossible de lire {}: {}", path, e);
                }
            }
        }
        Self::default()
    }

    pub fn is_cloud_mode(&self) -> bool {
        self.iso_path.is_none() || !Path::new(self.iso_path.as_deref().unwrap_or("")).exists()
    }
}
