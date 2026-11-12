//! App icons for the UI: the same icon Explorer shows for an .exe (or a Startup-folder shortcut),
//! returned as a small PNG data URL. Cached for the life of the process.
use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::HSTRING;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// The executable a command line starts: `"C:\a b\x.exe" --flag` → `C:\a b\x.exe`.
pub fn exe_from_command(cmd: &str) -> String {
    let c = crate::util::expand_env(cmd.trim());
    if let Some(r) = c.strip_prefix('"') {
        return r.split('"').next().unwrap_or("").to_string();
    }
    let low = c.to_ascii_lowercase();
    if let Some(i) = low.find(".exe") {
        return c[..i + 4].to_string();
    }
    c.split_whitespace().next().unwrap_or("").to_string()
}

unsafe fn bitmap_rgba(hbm: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let dc = GetDC(None);
    let n = GetDIBits(dc, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    (n == h).then_some(buf)
}

unsafe fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    GetIconInfo(icon, &mut ii).ok()?;
    let cleanup = |ii: &ICONINFO| {
        if !ii.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        }
        if !ii.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
        }
    };
    if ii.hbmColor.is_invalid() {
        cleanup(&ii); // monochrome icon: not worth drawing
        return None;
    }
    let mut bm = BITMAP::default();
    if GetObjectW(HGDIOBJ(ii.hbmColor.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
        cleanup(&ii);
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let color = bitmap_rgba(ii.hbmColor, w, h);
    // old icons have no alpha channel: transparency comes from the AND mask instead
    let mask = bitmap_rgba(ii.hbmMask, w, h);
    cleanup(&ii);
    let mut px = color?;
    let has_alpha = px.chunks(4).any(|p| p[3] != 0);
    for (i, p) in px.chunks_mut(4).enumerate() {
        p.swap(0, 2); // BGRA → RGBA
        if !has_alpha {
            let transparent = mask.as_ref().is_some_and(|m| m[i * 4] != 0);
            p[3] = if transparent { 0 } else { 255 };
        }
    }
    Some((w as u32, h as u32, px))
}

fn png_data_url(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64(&out)))
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn extract(path: &str) -> Option<String> {
    if path.is_empty() || !std::path::Path::new(path).exists() {
        return None;
    }
    unsafe {
        let mut info = SHFILEINFOW::default();
        let r = SHGetFileInfoW(
            &HSTRING::from(path),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if r == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let px = icon_rgba(info.hIcon);
        let _ = DestroyIcon(info.hIcon);
        let (w, h, rgba) = px?;
        png_data_url(w, h, &rgba)
    }
}

/// Icons for many paths at once (None where a file has no usable icon).
pub fn icons(paths: &[String]) -> HashMap<String, Option<String>> {
    // shortcuts (.lnk) need COM on this thread; harmless if it was already initialised
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let mut out = HashMap::new();
    for p in paths.iter().take(400) {
        let key = p.to_ascii_lowercase();
        if let Some(v) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&key).cloned())) {
            out.insert(p.clone(), v);
            continue;
        }
        let v = extract(p);
        if let Ok(mut c) = CACHE.lock() {
            c.get_or_insert_with(HashMap::new).insert(key, v.clone());
        }
        out.insert(p.clone(), v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        assert_eq!(exe_from_command(r#""C:\Program Files\A B\x.exe" --svc"#), r"C:\Program Files\A B\x.exe");
        assert_eq!(exe_from_command(r"C:\Program Files\A\x.exe -s"), r"C:\Program Files\A\x.exe");
        assert_eq!(exe_from_command("tool --x"), "tool");
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn explorer_has_an_icon() {
        let p = crate::util::expand_env(r"%SystemRoot%\explorer.exe");
        let r = icons(&[p.clone()]);
        assert!(r[&p].as_deref().is_some_and(|s| s.starts_with("data:image/png;base64,")));
    }
}

#[cfg(test)]
mod live {
    /// Writes a few real icons to %TEMP%\rigsweep-icons for a visual check.
    /// Run with: cargo test dump_icons -- --ignored
    #[test]
    #[ignore]
    fn dump_icons() {
        let dir = std::env::temp_dir().join("rigsweep-icons");
        let _ = std::fs::create_dir_all(&dir);
        let paths = [
            r"C:\Program Files\Fortinet\FortiClient\scheduler.exe",
            r"C:\Program Files (x86)\AnyDesk\AnyDesk.exe",
            r"C:\Windows\explorer.exe",
            r"C:\Windows\notepad.exe",
        ];
        for (i, p) in paths.iter().enumerate() {
            if let Some(Some(url)) = super::icons(&[p.to_string()]).get(*p) {
                let b64 = url.trim_start_matches("data:image/png;base64,");
                let html = format!("<img src=\"data:image/png;base64,{b64}\">");
                std::fs::write(dir.join(format!("{i}.html")), html).unwrap();
                println!("{p}: {} bytes", b64.len());
            } else {
                println!("{p}: none");
            }
        }
    }
}
