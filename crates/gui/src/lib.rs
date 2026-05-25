use std::fmt::Display;

pub mod backend;

use ecow::EcoString;
use egui::{Ui, WidgetText};
use serde::{Deserialize, Serialize};
pub mod tabs;

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

impl PanelId {
    pub fn new(backend: BackendId, panel: BackendPanelId) -> Self {
        Self { backend, panel }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BackendId(pub EcoString);

impl Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait Backend {
    fn name(&self) -> WidgetText;
    fn panels(&self) -> Vec<BackendPanelInfo>;
    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BackendPanelId(pub EcoString);

impl Display for BackendPanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
}

