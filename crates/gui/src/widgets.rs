use egui::Ui;


pub fn my_table(ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        egui::Grid::new("my_table")
        .striped(true)
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Row 1, Column 1");
            ui.label("Row 1, Column 2");
            ui.end_row();

            ui.label("Row 2, Column 1");
            ui.label("Row 2, Column 2");
            ui.end_row();

            ui.horizontal(|ui| {
                ui.label("aa");
                ui.take_available_width();
            });
            ui.horizontal(|ui| {
                ui.label("aa");
                ui.take_available_width();
            });
            ui.end_row();
        });
    });
}
