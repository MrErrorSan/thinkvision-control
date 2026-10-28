#![windows_subsystem = "windows"]
use wxdragon::prelude::*;
fn main() {
    let dark = std::env::args().any(|a| a == "--dark");
    let _ = wxdragon::main(move |_| {
        if dark { let _ = wxdragon::app::set_appearance(Appearance::Dark); }
        let frame = Frame::builder().with_title("ThinkVision Control").with_size(Size::new(1022, 727)).build();
        frame.show(true);
    });
}
