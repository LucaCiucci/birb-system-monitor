use std::collections::HashMap;

use birb_system_monitor_gui::{Backend, BackendId, BackendPanel, PanelId, backend::sysinfo::SysinfoBackend, tabs::{Tab, default_dock_state}};
use eframe::egui;
use egui::{MenuBar, Ui, WidgetText, accesskit::Uuid};
use egui_dock::{DockArea, TabViewer};

fn main() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(MonitorApp::new(cc)))),
    ).map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

struct MonitorApp {
    backends: HashMap<BackendId, Box<dyn Backend>>,
    panels: HashMap<Uuid, Box<dyn BackendPanel>>,
    dock_state: egui_dock::DockState<Tab>,
}

impl MonitorApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let dock_state = _cc.storage
            .and_then(|storage| storage.get_string("dock_state"))
            .and_then(|dock_json| serde_json::from_str(&dock_json).ok())
            .unwrap_or_else(|| default_dock_state());

        let mut backends = HashMap::<BackendId, Box<dyn Backend>>::default();

        backends.insert(
            BackendId("sysinfo".into()),
            Box::new(SysinfoBackend::new(_cc.egui_ctx.clone())),
        );

        Self { backends, panels: HashMap::default(), dock_state }
    }

    fn menu(&mut self, ui: &mut Ui) {
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    ui.close();
                }
            });

            ui.menu_button("view", |ui| {
                if ui.button("Reset layout").clicked() {
                    self.dock_state = default_dock_state();
                }
            });

            ui.menu_button("panels", |ui| {
                for (id, backend) in &mut self.backends {
                    ui.menu_button(backend.name(), |ui| {
                        for panel in (&**backend).panels() {
                            if ui.button(panel.title.as_str()).clicked() {
                                let panel_id = PanelId::new(id.clone(), panel.id.clone());
                                self.dock_state.push_to_focused_leaf(Tab::Panel(panel_id, Uuid::new_v4()));
                            }
                        }
                    });
                }
            })
        });
    }
}

impl eframe::App for MonitorApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.menu(ui);

            DockArea::new(&mut self.dock_state)
                .style(egui_dock::Style::from_egui(ui.style().as_ref()))
                .show_inside(ui, &mut MyTabViewer::new(&self.backends, &mut self.panels));
        });
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let dock_json = serde_json::to_string(&self.dock_state).unwrap();
        storage.set_string("dock_state", dock_json);
    }
}


struct MyTabViewer<'a> {
    backends: &'a HashMap<BackendId, Box<dyn Backend>>,
    panels: &'a mut HashMap<Uuid, Box<dyn BackendPanel>>,
}

impl<'a> MyTabViewer<'a> {
    fn new(
        backends: &'a HashMap<BackendId, Box<dyn Backend>>,
        panels: &'a mut HashMap<Uuid, Box<dyn BackendPanel>>,
    ) -> Self {
        Self { backends, panels }
    }

    fn get_panel(&mut self, panel_id: &PanelId, uuid: &Uuid) -> &mut dyn BackendPanel {
        self.panels.entry(*uuid).or_insert_with(|| {
            let backend = self.backends.get(&panel_id.backend).expect("Backend not found");
            backend.new_panel(&panel_id.panel)
        }).as_mut()
    }
}

impl<'a> TabViewer for MyTabViewer<'a> {
    // This associated type is used to attach some data to each tab.
    type Tab = Tab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        match tab {
            Tab::Panel(id, uuid) => egui::Id::new((id.clone(), *uuid)),
            Tab::Other(name) => egui::Id::new(name.clone()),
        }
    }

    // Returns the current `tab`'s title.
    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        match tab {
            Tab::Panel(id, _) => format!("Panel: {id}").into(),
            Tab::Other(name) => format!("Other: {name}").into(),
        }
    }

    // Defines the contents of a given `tab`.
    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Panel(id, uuid) => {
                let panel = self.get_panel(id, uuid);
                panel.ui(ui);
            }
            Tab::Other(name) => {
                ui.label(format!("This is the {name} tab"));
            }
        }
    }
}
