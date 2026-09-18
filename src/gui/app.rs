use std::{collections::HashMap, time::Duration};

use crate::gui::panels::PanelId;

use super::{
    Panel,
    backend::{Connection, FrontendState},
    save::Profile,
    tabs::{Tab, default_dock_state},
    widgets::placeholder_sentence,
};
use eframe::egui;
use egui::{Button, Color32, Id, MenuBar, Ui, Vec2, WidgetText, accesskit::Uuid};
use egui_dock::{DockArea, TabViewer};
use itertools::Itertools;
use ordered_hash_map::OrderedHashMap;
use tracing::info;

mod icon;

pub fn main(ssh: Option<String>, ssh_bin: String) -> anyhow::Result<()> {
    let app_id = env!("CARGO_PKG_NAME");

    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options
        .viewport
        .with_app_id(app_id.to_string())
        .with_icon(icon::make_icon());

    let save_path = eframe::storage_dir(app_id);
    info!("Save path: {:?}", save_path);

    eframe::run_native(
        "Birb System Monitor",
        native_options,
        Box::new(move |cc| Ok(Box::new(MonitorApp::new(cc, ssh.clone(), ssh_bin.clone())?))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

struct MonitorApp {
    ssh: Option<String>,
    ssh_bin: String,
    connection_error: Option<String>,
    connection: Option<Connection>,
    loaded_profile: Option<Profile>,
    frontend: FrontendState,
    panels: HashMap<(PanelId, Uuid), Box<dyn Panel>>,
    dock_states: OrderedHashMap<String, egui_dock::DockState<Tab>>,
    selected_tab: String,
}

impl MonitorApp {
    fn reset(&mut self, cx: &egui::Context) {
        self.connection.take();
        self.loaded_profile = None;
        self.frontend = FrontendState::new();
        match Connection::connect(
            cx.clone(),
            &self.frontend,
            self.ssh.as_deref(),
            &self.ssh_bin,
        ) {
            Ok(connection) => {
                self.connection = Some(connection);
                self.connection_error = None;
            }
            Err(error) => self.connection_error = Some(error.to_string()),
        }
        self.panels.clear();
        self.dock_states = Default::default();
        self.selected_tab = "main".into();
    }

    fn new(
        _cc: &eframe::CreationContext<'_>,
        ssh: Option<String>,
        ssh_bin: String,
    ) -> anyhow::Result<Self> {
        let loaded_profile: Option<Profile> = _cc
            .storage
            .and_then(|storage| storage.get_string("profile"))
            .and_then(|profile_json| {
                serde_json::from_str(&profile_json)
                    .map_err(|e| {
                        eprintln!("Failed to parse profile JSON: {e}");
                        eprintln!("Profile JSON was: {profile_json}");
                        e
                    })
                    .ok()
            });

        let dock_states = loaded_profile
            .as_ref()
            .map(|p| p.dock_states.clone())
            .unwrap_or_default();

        let frontend = FrontendState::new();
        if let Some(profile) = &loaded_profile {
            frontend.apply_config(&profile.frontend_config);
        }

        // Eagerly create all panels from saved dock layout so configs are loaded
        // even for panels in tabs that haven't been viewed yet.
        let mut panels: HashMap<(PanelId, Uuid), Box<dyn Panel>> = HashMap::new();
        for dock_state in dock_states.values() {
            for (_path, tab) in dock_state.iter_all_tabs() {
                if let Tab::Panel(panel_id, uuid) = tab {
                    let mut panel = frontend.new_panel(panel_id);
                    if let Some(config) = loaded_profile
                        .as_ref()
                        .and_then(|p| p.get_panel_config(panel_id, uuid))
                    {
                        if let Err(e) = panel.load_config(config) {
                            eprintln!("Failed to load config for panel {}: {:?}", panel_id, e);
                        }
                    }
                    panels.insert((*panel_id, *uuid), panel);
                }
            }
        }

        let connection = Some(Connection::connect(
            _cc.egui_ctx.clone(),
            &frontend,
            ssh.as_deref(),
            &ssh_bin,
        )?);
        Ok(Self {
            ssh,
            ssh_bin,
            connection_error: None,
            connection,
            loaded_profile,
            frontend,
            panels,
            dock_states,
            selected_tab: "main".into(),
        })
    }

    fn menu(&mut self, ui: &mut Ui) {
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    ui.close();
                }

                if ui.button("Save profile").clicked() {
                    if let Some(profile) = &mut self.loaded_profile {
                        let file = rfd::FileDialog::new()
                            .set_title("Save Profile")
                            .add_filter("JSON", &["json"])
                            .add_filter("YAML", &["yaml", "yml"])
                            .add_filter("TOML", &["toml"])
                            .add_filter("HJSON", &["hjson"])
                            .add_filter("RON", &["ron"])
                            .save_file();
                        if let Some(file) = file {
                            if let Err(e) = profile.save(file) {
                                eprintln!("Failed to save profile: {:?}", e);
                            }
                        }
                    }
                }
            });

            ui.menu_button("view", |ui| {
                if ui.button("Reset layout").clicked() {
                    self.reset(ui.ctx());
                }
            });

            ui.menu_button("panels", |ui| {
                for (name, panels) in [
                    (self.frontend.sysinfo.name(), self.frontend.sysinfo.panels()),
                    (self.frontend.docker.name(), self.frontend.docker.panels()),
                ] {
                    ui.menu_button(name, |ui| {
                        for panel in panels {
                            if ui.button(panel.title.as_str()).clicked() {
                                self.dock_states
                                    .entry(self.selected_tab.clone())
                                    .or_insert_with(default_dock_state)
                                    .push_to_focused_leaf(Tab::Panel(panel.id, Uuid::new_v4()));
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
        if let Some(host) = &self.ssh {
            ui.label(format!("Remote: {host}"));
        }
        if let Some(error) = &self.connection_error {
            ui.colored_label(Color32::RED, error);
        }
        if let Some(connection) = &mut self.connection {
            connection.sync_config(&self.frontend);
            let status = connection.status.lock();
            if status.disconnected {
                ui.colored_label(
                    Color32::RED,
                    "Backend disconnected — displayed readings are stale. Restart to reconnect.",
                );
            }
            if let Some(error) = &status.error {
                ui.colored_label(Color32::RED, error);
            }
        }
        // Retry configuration sends if a bounded command queue was temporarily full.
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                placeholder_sentence(ui);
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            self.menu(ui);
            ui.separator();
            ui.horizontal(|ui| {
                for tab in self.dock_states.keys().cloned().collect_vec() {
                    let selected = self.selected_tab == *tab;
                    let selectable_label =
                        Button::selectable(selected, tab.as_str()).min_size(Vec2::new(50.0, 0.0));
                    if ui.add(selectable_label).clicked() {
                        self.selected_tab = tab.clone();
                    }
                    if selected {
                        let btn = Button::new("x").fill(Color32::DARK_RED).small();
                        if ui.add(btn).clicked() {
                            self.dock_states.remove(&tab);
                            if self.selected_tab == *tab {
                                self.selected_tab = self
                                    .dock_states
                                    .keys()
                                    .next()
                                    .cloned()
                                    .unwrap_or_else(|| "main".into());
                            }
                        }
                    }
                }
                {
                    let editing_id = Id::new("editing").with(ui.id());
                    let editing = ui
                        .data_mut(|m| m.get_temp_mut_or_insert_with(editing_id, || false).clone());
                    if editing {
                        let new_name_id = Id::new("editing_name").with(ui.id());
                        let mut new_name_str = ui.data_mut(|m| {
                            m.get_temp_mut_or_insert_with(new_name_id, || {
                                format!("tab_{}", self.dock_states.len() + 1)
                            })
                            .clone()
                        });
                        let new_name = ui.text_edit_singleline(&mut new_name_str).lost_focus();
                        ui.data_mut(|m: &mut egui::util::IdTypeMap| {
                            m.insert_temp(new_name_id, new_name_str.clone());
                        });
                        if new_name {
                            self.selected_tab = new_name_str.clone();
                            ui.data_mut(|m| m.insert_temp(editing_id, false));
                        }
                    } else {
                        let btn = Button::new("+").small();
                        let r = ui.add(btn).on_hover_text("Add new tab");
                        if r.clicked() {
                            ui.data_mut(|m| m.insert_temp(editing_id, true));
                        }
                    }
                }
            });

            DockArea::new(
                self.dock_states
                    .entry(self.selected_tab.clone())
                    .or_insert_with(default_dock_state),
            )
            .style(egui_dock::Style::from_egui(ui.style().as_ref()))
            .show_inside(
                ui,
                &mut MyTabViewer::new(&self.loaded_profile, &self.frontend, &mut self.panels),
            );
        });
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::from_secs(5)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let mut profile = Profile::new(self.dock_states.clone());

        profile.frontend_config = self.frontend.config();

        for ((panel_id, uuid), panel) in &self.panels {
            if let Ok(config) = panel.save_config() {
                profile.set_panel_config(panel_id, uuid, config);
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
    frontend: &'a FrontendState,
    panels: &'a mut HashMap<(PanelId, Uuid), Box<dyn Panel>>,
}

impl<'a> MyTabViewer<'a> {
    fn new(
        loaded_profile: &'a Option<Profile>,
        frontend: &'a FrontendState,
        panels: &'a mut HashMap<(PanelId, Uuid), Box<dyn Panel>>,
    ) -> Self {
        Self {
            loaded_profile,
            frontend,
            panels,
        }
    }

    fn get_panel(&mut self, panel_id: &PanelId, uuid: &Uuid) -> &mut dyn Panel {
        self.panels
            .entry((panel_id.clone(), *uuid))
            .or_insert_with(|| {
                let mut panel = self.frontend.new_panel(panel_id);

                let config = self
                    .loaded_profile
                    .as_ref()
                    .and_then(|p| p.get_panel_config(panel_id, uuid));

                if let Some(config) = config {
                    if let Err(e) = panel.load_config(config) {
                        eprintln!("Failed to load config for panel {}: {:?}", panel_id, e);
                    }
                }

                panel
            })
            .as_mut()
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
            Tab::Panel(id, uuid) => self.get_panel(id, uuid).title(),
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

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        match tab {
            Tab::Panel(id, uuid) => {
                let panel = self.panels.get(&(id.clone(), *uuid));
                if let Some(panel) = panel {
                    panel.scroll_bars()
                } else {
                    [false, true]
                }
            }
            Tab::Other(_) => [false, true],
        }
    }
}
