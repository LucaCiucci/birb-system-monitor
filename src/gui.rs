use std::{fmt::Display, str::FromStr};

pub mod backend;

use ecow::EcoString;
use egui::{Ui, WidgetText};
use serde::{Deserialize, Serialize};
pub mod tabs;
pub mod widgets;
pub mod save;
pub mod gui_main;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PanelId {
    pub backend: BackendId,
    pub panel: BackendPanelId,
}

impl Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.backend, self.panel)
    }
}

impl FromStr for PanelId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid PanelId format: '{}'", s));
        }
        Ok(PanelId {
            backend: BackendId(parts[0].into()),
            panel: BackendPanelId(parts[1].into()),
        })
    }
}

impl PanelId {
    pub fn new(backend: BackendId, panel: BackendPanelId) -> Self {
        Self { backend, panel }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendId(pub EcoString);

impl Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for BackendId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BackendId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(BackendId(s.into()))
    }
}

pub trait Backend {
    fn name(&self) -> WidgetText;
    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }
    fn load_config(&mut self, _config: &serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }
    fn panels(&self) -> Vec<BackendPanelInfo>;
    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendPanelId(pub EcoString);

impl Display for BackendPanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for BackendPanelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BackendPanelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(BackendPanelId(s.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BackendPanelInfo {
    pub id: BackendPanelId,
    pub title: EcoString,
    pub description: EcoString,
}

pub trait BackendPanel {
    fn title(&mut self) -> WidgetText;
    fn ui(&mut self, ui: &mut Ui);
    fn scroll_bars(&self) -> [bool; 2] {
        [false, true]
    }
    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }
    fn load_config(&mut self, _config: &serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }
}

