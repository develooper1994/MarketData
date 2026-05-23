use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceMetadata {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub supported_asset_classes: Vec<String>,
    #[serde(default)]
    pub supported_datasets: Vec<String>,
    #[serde(default)]
    pub priority: Option<u8>,
    #[serde(default)]
    pub api_templates: Option<serde_json::Value>,
    #[serde(default)]
    pub health_probe: Option<String>,
}

#[derive(Debug, Default)]
pub struct SourceRegistry {
    map: HashMap<String, SourceMetadata>,
}

impl SourceRegistry {
    pub fn load_from_path(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let s = fs::read_to_string(path)?;
        let doc: RegistryDoc = serde_yaml::from_str(&s)?;
        let mut map = HashMap::new();
        for meta in doc.sources {
            map.insert(meta.id.clone(), meta);
        }
        Ok(SourceRegistry { map })
    }

    pub fn get(&self, id: &str) -> Option<&SourceMetadata> {
        self.map.get(id)
    }

    pub fn get_by_asset_class(&self, class: &str) -> Vec<&SourceMetadata> {
        self.map
            .values()
            .filter(|m| m.supported_asset_classes.iter().any(|c| c == class))
            .collect()
    }

    pub fn all(&self) -> Vec<&SourceMetadata> {
        self.map.values().collect()
    }
}

#[derive(Debug, Deserialize)]
struct RegistryDoc {
    sources: Vec<SourceMetadata>,
}
