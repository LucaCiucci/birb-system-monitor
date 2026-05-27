use std::{collections::HashMap, time::Duration};

use crate::{Backend, BackendId, BackendPanel, PanelId, backend::init_all_backends, save::Profile, tabs::{Tab, default_dock_state}, widgets::placeholder_sentence};
use eframe::egui;
use egui::{Button, Color32, Id, MenuBar, Ui, Vec2, WidgetText, accesskit::Uuid};
use egui_dock::{DockArea, TabViewer};
use itertools::Itertools;
use ordered_hash_map::OrderedHashMap;
use tracing::info;

fn make_icon() -> egui::IconData {
    let size = 32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    // Define a few polyline segments for the graph lines
    let lines: &[&[(i32, i32)]] = &[
        &[(4, 24), (12, 20), (20, 22), (28, 12)],  // CPU - blue
        &[(4, 24), (12, 22), (20, 18), (28, 16)],  // Mem - green
        &[(4, 24), (12, 23), (20, 20), (28, 18)],  // Net - orange
    ];
    let colors: &[(u8, u8, u8)] = &[
        (100, 181, 246),
        (129, 199, 132),
        (255, 183, 77),
    ];

    for y in 0..size {
        for x in 0..size {
            let (r, g, b, a) = if x >= 4 && x <= 28 && y >= 6 && y <= 28 {
                let px = x as f32;
                let py = y as f32;
                let mut closest = f32::MAX;
                let mut best = (20u8, 20u8, 30u8);

                for (line_idx, seg) in lines.iter().enumerate() {
                    for pair in seg.windows(2) {
                        let (x1, y1) = pair[0];
                        let (x2, y2) = pair[1];
                        let dx = (x2 - x1) as f32;
                        let dy = (y2 - y1) as f32;
                        let len2 = dx * dx + dy * dy;
                        if len2 < 0.001 { continue; }
                        let t = ((px - x1 as f32) * dx + (py - y1 as f32) * dy) / len2;
                        let t = t.clamp(0.0, 1.0);
                        let nx = x1 as f32 + t * dx;
                        let ny = y1 as f32 + t * dy;
                        let d = ((px - nx).powi(2) + (py - ny).powi(2)).sqrt();
                        if d < closest {
                            closest = d;
                            best = colors[line_idx];
                        }
                    }
                }

                if closest < 2.5 {
                    (best.0, best.1, best.2, 255)
                } else {
                    let bg = 20 + ((y as f32 / size as f32) * 15.0) as u8;
                    (bg, bg, bg + 10, 255)
                }
            } else {
                (10, 10, 20, 255)
            };
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }
    egui::IconData { rgba, width: size, height: size }
}

pub fn main() -> anyhow::Result<()> {
    let app_id = env!("CARGO_PKG_NAME");

    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = native_options.viewport
        .with_app_id(app_id.to_string())
        .with_icon(make_icon());

    let save_path = eframe::storage_dir(app_id);
    info!("Save path: {:?}", save_path);

    eframe::run_native(
        "Birb System Monitor",
        native_options,
        Box::new(|cc| Ok(Box::new(MonitorApp::new(cc)))),
    ).map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

struct MonitorApp {
    loaded_profile: Option<Profile>,
    backends: HashMap<BackendId, Box<dyn Backend>>,
    panels: HashMap<(PanelId, Uuid), Box<dyn BackendPanel>>,
    dock_states: OrderedHashMap<String, egui_dock::DockState<Tab>>,
    selected_tab: String,
}

impl MonitorApp {
    fn reset(&mut self, cx: &egui::Context) {
        self.loaded_profile = None;
        self.backends = init_all_backends(cx);
        self.panels.clear();
        self.dock_states = Default::default();
        self.selected_tab = "main".into();
    }

    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let loaded_profile: Option<Profile> = _cc.storage
            .and_then(|storage| storage.get_string("profile"))
            .and_then(|profile_json| serde_json::from_str(&profile_json).map_err(|e| {
                eprintln!("Failed to parse profile JSON: {e}");
                eprintln!("Profile JSON was: {profile_json}");
                e
            }).ok());

        let dock_states = loaded_profile.as_ref().map(|p| p.dock_states.clone()).unwrap_or_default();

        let mut backends = init_all_backends(&_cc.egui_ctx);

        for (id, backend) in &mut backends {
            if let Some(config) = loaded_profile.as_ref().and_then(|p| p.get_backend_config(id)) {
                if let Err(e) = backend.load_config(config) {
                    eprintln!("Failed to load config for backend {}: {:?}", id, e);
                }
            }
        }

        // Eagerly create all panels from saved dock layout so configs are loaded
        // even for panels in tabs that haven't been viewed yet.
        let mut panels: HashMap<(PanelId, Uuid), Box<dyn BackendPanel>> = HashMap::new();
        for dock_state in dock_states.values() {
            for (_path, tab) in dock_state.iter_all_tabs() {
                if let Tab::Panel(panel_id, uuid) = tab {
                    if let Some(backend) = backends.get(&panel_id.backend) {
                        let mut panel = backend.new_panel(&panel_id.panel);
                        if let Some(config) = loaded_profile.as_ref().and_then(|p| {
                            p.get_panel_config(&panel_id.backend, &panel_id.panel, uuid)
                        }) {
                            if let Err(e) = panel.load_config(config) {
                                eprintln!("Failed to load config for panel {}: {:?}", panel_id, e);
                            }
                        }
                        panels.insert((panel_id.clone(), *uuid), panel);
                    }
                }
            }
        }

        Self { loaded_profile, backends, panels, dock_states, selected_tab: "main".into() }
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
                                self.dock_states.entry(self.selected_tab.clone()).or_insert_with(default_dock_state).push_to_focused_leaf(Tab::Panel(panel_id, Uuid::new_v4()));
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
        egui::Panel::bottom("footer")
            .show_inside(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    placeholder_sentence(ui);
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.menu(ui);
            ui.separator();
            ui.horizontal(|ui| {
                for tab in self.dock_states.keys().cloned().collect_vec() {
                    let selected = self.selected_tab == *tab;
                    let selectable_label = Button::selectable(selected, tab.as_str()).min_size(Vec2::new(50.0, 0.0));
                    if ui.add(selectable_label).clicked() {
                        self.selected_tab = tab.clone();
                    }
                    if selected {
                        let btn = Button::new("x").fill(Color32::DARK_RED).small();
                        if ui.add(btn).clicked() {
                            self.dock_states.remove(&tab);
                            if self.selected_tab == *tab {
                                self.selected_tab = self.dock_states.keys().next().cloned().unwrap_or_else(|| "main".into());
                            }
                        }
                    }
                }
                {
                    let editing_id = Id::new("editing").with(ui.id());
                    let editing = ui.data_mut(|m| m.get_temp_mut_or_insert_with(editing_id, || false).clone());
                    if editing {
                        let new_name_id = Id::new("editing_name").with(ui.id());
                        let mut new_name_str = ui.data_mut(|m| m.get_temp_mut_or_insert_with(new_name_id, || format!("tab_{}", self.dock_states.len() + 1)).clone());
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

            DockArea::new(self.dock_states.entry(self.selected_tab.clone()).or_insert_with(default_dock_state))
                .style(egui_dock::Style::from_egui(ui.style().as_ref()))
                .show_inside(ui, &mut MyTabViewer::new(&self.loaded_profile, &self.backends, &mut self.panels));
        });
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        Duration::from_secs(1)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let mut profile = Profile::new(self.dock_states.clone());

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

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, true]
    }
}
