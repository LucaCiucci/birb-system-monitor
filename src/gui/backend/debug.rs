use crate::gui::{
    Backend, BackendPanel, BackendPanelId, BackendPanelInfo, widgets::placeholder_sentence,
};

pub struct DebugBackend;

impl Backend for DebugBackend {
    fn name(&self) -> egui::WidgetText {
        "Debug".into()
    }

    fn panels(&self) -> Vec<BackendPanelInfo> {
        vec![BackendPanelInfo {
            id: BackendPanelId("widget_gallery".into()),
            title: "Widget Gallery".into(),
            description: "A panel showcasing various widgets for testing and debugging.".into(),
        }]
    }

    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
        match panel_id.0.as_str() {
            "widget_gallery" => Box::new(WidgetGalleryPanel),
            _ => panic!("Unknown panel ID: {}", panel_id.0),
        }
    }
}

pub struct WidgetGalleryPanel;

impl BackendPanel for WidgetGalleryPanel {
    fn title(&mut self) -> egui::WidgetText {
        "Widget Gallery".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label("This is a widget gallery panel. You can add various widgets here for testing and debugging purposes.");
        placeholder_sentence(ui);
    }
}
