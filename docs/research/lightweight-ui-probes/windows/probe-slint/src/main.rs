#![windows_subsystem = "windows"]
slint::include_modules!();
fn main() {
    ProbeWindow::new().unwrap().run().unwrap();
}
