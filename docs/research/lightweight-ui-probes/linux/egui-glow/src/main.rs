// egui/eframe probe window: label, slider 0-100, checkbox (egui has no stock switch),
// combo (3 items), button.
use eframe::egui;

struct Probe {
    status: String,
    value: f32,
    enabled: bool,
    mode: usize,
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(&self.status);
            ui.add(egui::Slider::new(&mut self.value, 0.0..=100.0));
            ui.checkbox(&mut self.enabled, "Enabled");
            let items = ["Standard", "Movie", "Game"];
            egui::ComboBox::from_label("Mode")
                .selected_text(items[self.mode])
                .show_ui(ui, |ui| {
                    for (i, it) in items.iter().enumerate() {
                        ui.selectable_value(&mut self.mode, i, *it);
                    }
                });
            if ui.button("Apply").clicked() {
                self.status = "clicked".into();
            }
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("probe")
            .with_inner_size([600.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "probe",
        options,
        Box::new(|_cc| {
            Ok(Box::new(Probe { status: "Brightness".into(), value: 50.0, enabled: false, mode: 0 }))
        }),
    )
}
