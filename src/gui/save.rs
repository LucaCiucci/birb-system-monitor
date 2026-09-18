use std::{collections::BTreeMap, path::Path};

use anyhow::bail;
use egui_dock::DockState;
use ordered_hash_map::OrderedHashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::gui::{backend::FrontendConfig, panels::PanelId, tabs::Tab};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub dock_states: OrderedHashMap<String, DockState<Tab>>,
    pub frontend_config: FrontendConfig,
    pub panel_config: BTreeMap<PanelId, BTreeMap<String, serde_json::Value>>,
}

impl Profile {
    pub fn new(dock_states: OrderedHashMap<String, DockState<Tab>>) -> Self {
        Self {
            dock_states,
            frontend_config: Default::default(),
            panel_config: Default::default(),
        }
    }

    pub fn set_panel_config(&mut self, panel_id: &PanelId, uuid: &Uuid, config: serde_json::Value) {
        self.panel_config
            .entry(panel_id.clone())
            .or_default()
            .insert(uuid.to_string(), config);
    }

    pub fn get_panel_config(&self, panel_id: &PanelId, uuid: &Uuid) -> Option<&serde_json::Value> {
        self.panel_config
            .get(panel_id)
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
