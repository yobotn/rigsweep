//! Tray / taskbar indicator: a colored dot showing how much memory stale dev processes hold.

use std::sync::Mutex;

use tauri::image::Image;
use tauri::{AppHandle, Manager};

use crate::engine::Settings;

const ORANGE: [u8; 3] = [245, 158, 11];
const RED: [u8; 3] = [239, 68, 68];

/// last (level, tooltip) pushed to the shell, so the icon is only touched when it changes
static LAST: Mutex<Option<(u8, String)>> = Mutex::new(None);

/// 0 = nothing worth reporting, 1 = warn (orange), 2 = alert (red)
pub fn level(stale_mem: u64, s: &Settings) -> u8 {
    if !s.badge_enabled {
        return 0;
    }
    let gb = stale_mem as f64 / 1024f64.powi(3);
    if gb >= s.badge_alert_gb as f64 {
        2
    } else if gb >= s.badge_warn_gb as f64 {
        1
    } else {
        0
    }
}

/// Coverage (0..1) of a disc of radius `r` centered at (cx, cy) over pixel (x, y), roughly antialiased.
fn disc(x: u32, y: u32, cx: f32, cy: f32, r: f32) -> f32 {
    let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
    (r - d + 0.5).clamp(0.0, 1.0)
}

fn blend(px: &mut [u8], c: [u8; 3], a: f32) {
    if a <= 0.0 {
        return;
    }
    for i in 0..3 {
        px[i] = (px[i] as f32 * (1.0 - a) + c[i] as f32 * a).round() as u8;
    }
    px[3] = (px[3] as f32 + (255.0 - px[3] as f32) * a).round() as u8;
}

/// Paint a dot with a white ring at (cx, cy) onto an RGBA buffer.
fn paint_dot(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, color: [u8; 3]) {
    let ring = (r * 0.22).max(1.0);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let px = &mut rgba[i..i + 4];
            blend(px, [255, 255, 255], disc(x, y, cx, cy, r + ring));
            blend(px, color, disc(x, y, cx, cy, r));
        }
    }
}

/// The app icon with a dot in the bottom-right corner.
pub fn badged(base: &Image<'_>, color: [u8; 3]) -> Image<'static> {
    let (w, h) = (base.width(), base.height());
    let mut rgba = base.rgba().to_vec();
    let r = w.min(h) as f32 * 0.24;
    paint_dot(&mut rgba, w, h, w as f32 - r * 1.25, h as f32 - r * 1.25, r, color);
    Image::new_owned(rgba, w, h)
}

/// Just the dot, for the taskbar button overlay.
fn dot(color: [u8; 3]) -> Image<'static> {
    let n = 32u32;
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    paint_dot(&mut rgba, n, n, n as f32 / 2.0, n as f32 / 2.0, n as f32 * 0.38, color);
    Image::new_owned(rgba, n, n)
}

fn fmt_gb(b: u64) -> String {
    let gb = b as f64 / 1024f64.powi(3);
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", b as f64 / 1024f64.powi(2))
    }
}

/// Update the tray icon, its tooltip and the taskbar overlay for the current stale total.
pub fn update(app: &AppHandle, stale_mem: u64, s: &Settings) {
    let lvl = level(stale_mem, s);
    let tip = if s.badge_enabled && stale_mem > 0 {
        format!("RigSweep · {} reclaimable from stale dev processes", fmt_gb(stale_mem))
    } else {
        "RigSweep".to_string()
    };
    {
        let Ok(mut last) = LAST.lock() else { return };
        if last.as_ref() == Some(&(lvl, tip.clone())) {
            return;
        }
        *last = Some((lvl, tip.clone()));
    }
    let color = match lvl {
        2 => Some(RED),
        1 => Some(ORANGE),
        _ => None,
    };
    if let (Some(tray), Some(base)) = (app.tray_by_id("main"), app.default_window_icon()) {
        let icon = match color {
            Some(c) => badged(base, c),
            None => base.clone().to_owned(),
        };
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_tooltip(Some(&tip));
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_overlay_icon(color.map(dot));
    }
}

/// Forget what was last shown, so the next `update` repaints (e.g. after the window is re-shown).
pub fn invalidate() {
    if let Ok(mut last) = LAST.lock() {
        *last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(warn: f32, alert: f32) -> Settings {
        Settings {
            start_with_windows: false,
            stay_in_tray: true,
            watchdog_minutes: 30,
            dev_folders: vec![],
            idle_minutes: 120,
            auto_kill_stale: false,
            badge_enabled: true,
            badge_warn_gb: warn,
            badge_alert_gb: alert,
            auto_update: false,
            disk_roots: vec![],
            disk_excludes: vec![],
            disk_disabled: vec![],
            disk_stale_days: 30,
            disk_auto_build: true,
            disk_auto_deps: true,
            disk_auto_caches: true,
            disk_auto_min_mb: 50,
        }
    }

    #[test]
    fn levels_follow_thresholds() {
        let gb = 1024u64.pow(3);
        let s = settings(1.0, 2.0);
        assert_eq!(level(gb / 2, &s), 0);
        assert_eq!(level(gb, &s), 1);
        assert_eq!(level(2 * gb, &s), 2);
        let mut off = s.clone();
        off.badge_enabled = false;
        assert_eq!(level(5 * gb, &off), 0);
    }

    #[test]
    fn dot_is_painted_in_the_corner_only() {
        let base = Image::new_owned(vec![0; 32 * 32 * 4], 32, 32);
        let img = badged(&base, RED);
        let px = |x: u32, y: u32| img.rgba()[((y * 32 + x) * 4) as usize..][..4].to_vec();
        assert_eq!(px(2, 2), vec![0, 0, 0, 0]); // untouched top-left
        let c = 32.0 - 32.0 * 0.24 * 1.25;
        assert_eq!(px(c as u32, c as u32), vec![RED[0], RED[1], RED[2], 255]);
    }
}
