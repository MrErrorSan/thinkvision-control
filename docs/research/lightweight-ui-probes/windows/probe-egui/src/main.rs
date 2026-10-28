#![windows_subsystem = "windows"]
use eframe::egui;

struct App { v: f32, on: bool, sel: usize }
const OPTS: [&str; 3] = ["sRGB", "DCI-P3", "Custom"];

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label("Brightness");
            ui.add(egui::Slider::new(&mut self.v, 0.0..=100.0));
            ui.checkbox(&mut self.on, "Auto"); // egui has no built-in toggle switch
            egui::ComboBox::from_id_salt("preset").selected_text(OPTS[self.sel]).show_ui(ui, |ui| {
                for (i, o) in OPTS.iter().enumerate() { ui.selectable_value(&mut self.sel, i, *o); }
            });
            let _ = ui.button("Apply");
        });
    }
}

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("probe").with_inner_size([600.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native("probe", opts, Box::new(|_cc| Ok(Box::new(App { v: 50.0, on: false, sel: 0 }))))
}
