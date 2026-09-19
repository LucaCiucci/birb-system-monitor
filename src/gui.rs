pub mod backend;

use ecow::EcoString;
use egui::{Ui, WidgetText};
use serde::{Deserialize, Serialize};

use crate::gui::app::state::FrontendState;
pub mod app;
pub mod panels;
pub mod save;
pub mod tabs;
pub mod widgets;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PanelInfo {
    pub id: panels::PanelId,
    pub title: EcoString,
    pub description: EcoString,
}

pub trait Panel {
    fn title(&mut self) -> WidgetText;
    fn ui(&mut self, data: &mut FrontendState, ui: &mut Ui);
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
