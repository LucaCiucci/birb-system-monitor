use std::{collections::BTreeMap, path::Path};

use anyhow::bail;
use egui_dock::DockState;
use ordered_hash_map::OrderedHashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{BackendId, BackendPanelId, tabs::Tab};



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub dock_states: OrderedHashMap<String, DockState<Tab>>,
    pub backend_config: BTreeMap<BackendId, serde_json::Value>,
    pub panel_config: BTreeMap<BackendId, BTreeMap<BackendPanelId, BTreeMap<String, serde_json::Value>>>,
}

impl Profile {
    pub fn new(dock_states: OrderedHashMap<String, DockState<Tab>>) -> Self {
        Self {
            dock_states,
            backend_config: Default::default(),
            panel_config: Default::default(),
        }
    }

    pub fn set_backend_config(&mut self, backend_id: &BackendId, config: serde_json::Value) {
        self.backend_config.insert(backend_id.clone(), config);
    }

    pub fn get_backend_config(&self, backend_id: &BackendId) -> Option<&serde_json::Value> {
        self.backend_config.get(backend_id)
    }

    pub fn set_panel_config(&mut self, backend_id: &BackendId, panel_id: &BackendPanelId, uuid: &Uuid, config: serde_json::Value) {
        self.panel_config
            .entry(backend_id.clone())
            .or_default()
            .entry(panel_id.clone())
            .or_default()
            .insert(uuid.to_string(), config);
    }

    pub fn get_panel_config(&self, backend_id: &BackendId, panel_id: &BackendPanelId, uuid: &Uuid) -> Option<&serde_json::Value> {
        self.panel_config
            .get(backend_id)
            .and_then(|panels| panels.get(panel_id))
            .and_then(|configs| configs.get(&uuid.to_string()))
    }

    pub fn save(&self, file: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = file.as_ref();
        let ext = file.extension().and_then(|s| s.to_str()).unwrap_or("");
        match ext {
            "json" => {
                let json = serde_json::to_string_pretty(self)?;
                std::fs::write(file, json)?;
            }
            "yaml" | "yml" => {
                let yaml = serde_yaml::to_string(self)?;
                std::fs::write(file, yaml)?;
            }
            "toml" => {
                let toml = toml::to_string(self)?;
                std::fs::write(file, toml)?;
            }
            "hjson" => {
                let hjson = serde_hjson::to_string(self)?;
                std::fs::write(file, hjson)?;
            }
            "ron" => {
                let ron = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
                std::fs::write(file, ron)?;
            }
            _ => bail!("Unsupported file extension: {ext}"),
        }

        Ok(())
    }
}



