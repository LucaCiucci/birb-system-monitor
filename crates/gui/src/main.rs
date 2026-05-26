use std::{collections::HashMap, time::Duration};

use birb_system_monitor_gui::{Backend, BackendId, BackendPanel, PanelId, backend::init_all_backends, save::Profile, tabs::{Tab, default_dock_state}};
use eframe::egui;
use egui::{MenuBar, Ui, WidgetText, accesskit::Uuid};
use egui_dock::{DockArea, TabViewer};
use tracing::info;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    //env_logger::init();

    let app_id = env!("CARGO_PKG_NAME");

    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options.viewport
        .with_app_id(app_id.to_string());

    let save_path = eframe::storage_dir(app_id);
    info!("Save path: {:?}", save_path);

    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(MonitorApp::new(cc)))),
    ).map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

struct MonitorApp {
    loaded_profile: Option<Profile>,
    backends: HashMap<BackendId, Box<dyn Backend>>,
    panels: HashMap<(PanelId, Uuid), Box<dyn BackendPanel>>,
    dock_state: egui_dock::DockState<Tab>,
}

impl MonitorApp {
    fn reset(&mut self, cx: &egui::Context) {
        self.loaded_profile = None;
        self.backends = init_all_backends(cx);
        self.panels.clear();
        self.dock_state = default_dock_state();
    }

    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let s = _cc.storage.unwrap().get_string("profile").unwrap_or_default();
        eprintln!("Loaded profile string: {s}");

        let loaded_profile: Option<Profile> = _cc.storage
            .and_then(|storage| storage.get_string("profile"))
            .and_then(|profile_json| serde_json::from_str(&profile_json).map_err(|e| {
                eprintln!("Failed to parse profile JSON: {e}");
                eprintln!("Profile JSON was: {profile_json}");
                e
            }).ok());

        eprintln!("Loaded profile: {:#?}", loaded_profile);

        let dock_state = loaded_profile.as_ref().map(|p| p.dock_state.clone()).unwrap_or_else(default_dock_state);

        let mut backends = init_all_backends(&_cc.egui_ctx);

        for (id, backend) in &mut backends {
            if let Some(config) = loaded_profile.as_ref().and_then(|p| p.get_backend_config(id)) {
                if let Err(e) = backend.load_config(config) {
                    eprintln!("Failed to load config for backend {}: {:?}", id, e);
                }
            }
        }

        Self { loaded_profile, backends, panels: HashMap::default(), dock_state }
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
                    self.reset(ui.ctx());
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
                .show_inside(ui, &mut MyTabViewer::new(&self.loaded_profile, &self.backends, &mut self.panels));
        });
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::from_secs(1)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let mut profile = Profile::new(self.dock_state.clone());

        for (id, backend) in &self.backends {
            if let Ok(config) = backend.save_config() {
                profile.set_backend_config(id, config);
            }
        }

        for ((panel_id, uuid), panel) in &self.panels {
            if let Ok(config) = panel.save_config() {
                profile.set_panel_config(&panel_id.backend, &panel_id.panel, uuid, config);
            }
        }

        //profile.save("profile.json").unwrap();
        //profile.save("profile.yaml").unwrap();
        //profile.save("profile.hjson").unwrap();
        ////profile.save("profile.toml").unwrap();
        //profile.save("profile.ron").unwrap();

        //eprintln!("Saving profile: {:#?}", profile);

        let json = serde_json::to_string(&profile).unwrap();
        storage.set_string("profile", json);

        //let dock_json = serde_json::to_string(&self.dock_state).unwrap();
        //storage.set_string("dock_state", dock_json);
    }
}


struct MyTabViewer<'a> {
    loaded_profile: &'a Option<Profile>,
    backends: &'a HashMap<BackendId, Box<dyn Backend>>,
    panels: &'a mut HashMap<(PanelId, Uuid), Box<dyn BackendPanel>>,
}

impl<'a> MyTabViewer<'a> {
    fn new(
        loaded_profile: &'a Option<Profile>,
        backends: &'a HashMap<BackendId, Box<dyn Backend>>,
        panels: &'a mut HashMap<(PanelId, Uuid), Box<dyn BackendPanel>>,
    ) -> Self {
        Self { loaded_profile, backends, panels }
    }

    fn get_panel(&mut self, panel_id: &PanelId, uuid: &Uuid) -> &mut dyn BackendPanel {
        self.panels.entry((panel_id.clone(), *uuid)).or_insert_with(|| {
            let backend = self.backends.get(&panel_id.backend).expect("Backend not found");
            let mut panel = backend.new_panel(&panel_id.panel);

            let config = self.loaded_profile
                .as_ref()
                .and_then(|p| p.get_panel_config(&panel_id.backend, &panel_id.panel, uuid));

            if let Some(config) = config {
                if let Err(e) = panel.load_config(config) {
                    eprintln!("Failed to load config for panel {}: {:?}", panel_id, e);
                }
            }

            panel
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
