use egui::{Color32, Grid, Ui, WidgetText};

use crate::gui::{Panel, app::state::FrontendState};

pub struct ImagesPanel;

impl ImagesPanel {
    pub fn new() -> Self {
        Self
    }
}

fn format_size(bytes: i64) -> String {
    if bytes < 0 {
        return "unknown".into();
    }
    let bytes = bytes as f64;
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut val = bytes;
    let mut unit_idx = 0;
    while val >= 1024.0 && unit_idx < UNITS.len() - 1 {
        val /= 1024.0;
        unit_idx += 1;
    }
    format!("{:.1} {}", val, UNITS[unit_idx])
}

impl Panel for ImagesPanel {
    fn title(&mut self) -> WidgetText {
        "Images".into()
    }

    fn ui(&mut self, data: &mut FrontendState, ui: &mut Ui) {
        let data = &data.docker.state;

        if let Some(ref err) = data.state.images_error {
            ui.colored_label(Color32::LIGHT_RED, format!("⚠ {err}"));
            return;
        }

        if !data.state.connected {
            ui.spinner();
            ui.label("Connecting to Docker...");
            return;
        }

        if data.state.images.is_empty() {
            ui.label("No images found.");
            return;
        }

        egui::ScrollArea::both().show(ui, |ui| {
            Grid::new("docker_images")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    // Headers
                    ui.strong("ID");
                    ui.strong("Repository");
                    ui.strong("Tag");
                    ui.strong("Size");
                    ui.strong("Created");
                    ui.end_row();

                    for image in &data.state.images {
                        let short_id = if image.id.len() >= 12 {
                            // Strip "sha256:" prefix if present
                            let id = image.id.strip_prefix("sha256:").unwrap_or(&image.id);
                            if id.len() >= 12 { &id[..12] } else { id }
                        } else {
                            &image.id
                        };

                        // Show first repo:tag, or <none> if empty
                        let (repo, tag) = if image.repo_tags.is_empty() {
                            ("<none>".to_string(), "<none>".to_string())
                        } else {
                            let first = &image.repo_tags[0];
                            if let Some((r, t)) = first.rsplit_once(':') {
                                (r.to_string(), t.to_string())
                            } else {
                                (first.clone(), "<none>".to_string())
                            }
                        };

                        ui.label(short_id);
                        ui.label(&repo);
                        ui.label(&tag);
                        ui.label(format_size(image.size));
                        ui.label(format_created(image.created));
                        ui.end_row();
                    }
                });
        });
    }

    fn scroll_bars(&self) -> [bool; 2] {
        [true, true]
    }
}

fn format_created(timestamp: i64) -> String {
    if timestamp == 0 {
        return "unknown".into();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let diff = now - timestamp;
    if diff < 0 {
        return "in the future".into();
    }

    let seconds = diff;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 365 {
        format!("{} years ago", days / 365)
    } else if days > 30 {
        format!("{} months ago", days / 30)
    } else if days > 0 {
        format!("{} days ago", days)
    } else if hours > 0 {
        format!("{} hours ago", hours)
    } else if minutes > 0 {
        format!("{} minutes ago", minutes)
    } else {
        format!("{} seconds ago", seconds)
    }
}
