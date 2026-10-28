//! Tray icon image: the glyph for the taskbar theme, with a status badge when the monitor isn't reachable.

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use tvc_core::engine::MonitorStatus;

use crate::Tray;

pub fn tooltip(status: MonitorStatus, short_name: &str) -> String {
    let state = match status {
        MonitorStatus::Connected => short_name,
        MonitorStatus::Connecting => "Connecting",
        MonitorStatus::NotFound => "Not connected",
        MonitorStatus::NotResponding => "Not responding",
    };
    format!("ThinkVision Control – {state}")
}

pub fn icon(tray: &Tray, status: MonitorStatus, taskbar_light: bool) -> Image {
    let base = if taskbar_light { tray.get_glyph_on_light() } else { tray.get_glyph_on_dark() };
    let badge = match status {
        MonitorStatus::NotFound => Badge::Cross,
        MonitorStatus::NotResponding => Badge::Bang,
        _ => return base,
    };
    let Some(mut buf) = base.to_rgba8() else { return base };
    draw_badge(&mut buf, badge, taskbar_light);
    Image::from_rgba8(buf)
}

#[derive(Clone, Copy)]
enum Badge {
    Cross,
    Bang,
}

/// A filled circle in the bottom-right corner, cut out of the glyph with a 1 px transparent ring, with a mark.
/// Colours follow 0.0.4: × on a foreground-coloured circle, ! on a caution circle; mark contrasts with the taskbar.
fn draw_badge(buf: &mut SharedPixelBuffer<Rgba8Pixel>, badge: Badge, light: bool) {
    let (w, h) = (buf.width() as f32, buf.height() as f32);
    let d = (0.45 * w).round().max(7.0);
    let r = d / 2.0;
    let (cx, cy) = (w - r, h - r);
    let fill = match (badge, light) {
        (Badge::Cross, false) => [255, 255, 255],
        (Badge::Cross, true) => [0x1b, 0x1b, 0x1b],
        (Badge::Bang, false) => [0xfc, 0xe1, 0x00],
        (Badge::Bang, true) => [0x9d, 0x5d, 0x00],
    };
    let mark = if light { [255, 255, 255] } else { [0, 0, 0] };
    let width = buf.width() as usize;
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = ((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
        let (dx, dy) = (x - cx, y - cy);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > r + 1.0 {
            continue;
        }
        if dist > r {
            *px = Rgba8Pixel { r: 0, g: 0, b: 0, a: 0 };
            continue;
        }
        let on_mark = match badge {
            Badge::Cross => {
                let k = r * 0.45;
                dx.abs() <= k && dy.abs() <= k && ((dx - dy).abs() < 1.1 || (dx + dy).abs() < 1.1)
            }
            Badge::Bang => dx.abs() < 0.9 && ((dy > -r * 0.6 && dy < r * 0.15) || (dy > r * 0.35 && dy < r * 0.62)),
        };
        let c = if on_mark { mark } else { fill };
        *px = Rgba8Pixel { r: c[0], g: c[1], b: c[2], a: 255 };
    }
}
