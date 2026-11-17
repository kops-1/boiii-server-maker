//! Pictures for the map list.
//!
//! * Workshop maps ship a `previewimage.png` next to their files.
//! * Official maps have none, so a frame is taken from the map's own loading
//!   movie in the game's `video` folder, through the same Windows service
//!   that makes Explorer's thumbnails, and cached in the data folder.
//!
//! Nothing is bundled or downloaded.

use bsm_core::content::{Item, Kind};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::core::GUID;
use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::UI::Shell::SHCreateItemFromParsingName;

const IID_ISHELLITEMIMAGEFACTORY: GUID = GUID {
    data1: 0xbcc18b79,
    data2: 0xba16,
    data3: 0x442f,
    data4: [0x80, 0xc4, 0x8a, 0x59, 0xc3, 0x0c, 0x46, 0x3b],
};
const SIIGBF_THUMBNAILONLY: i32 = 0x08;

pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("thumbs")
}

/// The loading movie of an official map, e.g. `zm_zod_load_zodloadingmovie.mkv`.
pub fn find_video(game_dir: &Path, map: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(game_dir.join("video"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name.starts_with(&map.to_lowercase()) && name.ends_with(".mkv")
        })
        .collect();
    // Prefer the loading movie, avoid outros, then the shortest name.
    found.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        (!name.contains("loadingmovie"), name.contains("outro"), name.len())
    });
    found.into_iter().next()
}

/// Picture file for a map, if there is one already. Never does slow work.
pub fn existing(item: &Item, data_dir: &Path) -> Option<PathBuf> {
    match item.kind {
        Kind::WorkshopMap | Kind::Mod => ["previewimage.png", "preview.jpg", "loadingimage.png"]
            .iter()
            .map(|n| item.path.join(n))
            .find(|p| p.is_file()),
        Kind::OfficialMap => {
            // A picture the user put there wins: <data>\thumbs\<map>.png
            let p = cache_dir(data_dir).join(format!("{}.png", item.name));
            p.is_file().then_some(p)
        }
    }
}

/// Makes the cached picture of an official map from its loading movie.
/// Slow (Windows decodes the movie); call from a worker thread.
pub fn make_official(map: &str, game_dir: &Path, data_dir: &Path) -> Option<PathBuf> {
    let video = find_video(game_dir, map)?;
    let (width, height, rgba) = shell_thumbnail(&video, 480, 270)?;
    let dir = cache_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(format!("{map}.png"));
    image::save_buffer(&out, &rgba, width, height, image::ColorType::Rgba8).ok()?;
    Some(out)
}

#[repr(C)]
struct ImageFactoryVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    // SIZE is two 32-bit values; on x64 a struct of 8 bytes is passed as one 64-bit value.
    get_image: unsafe extern "system" fn(*mut c_void, u64, i32, *mut *mut c_void) -> i32,
}

/// Asks the Windows shell for a thumbnail of a file. Returns (width, height, RGBA).
fn shell_thumbnail(file: &Path, width: u32, height: u32) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = file.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let result = (|| {
            let mut factory: *mut c_void = std::ptr::null_mut();
            if SHCreateItemFromParsingName(
                wide.as_ptr(),
                std::ptr::null_mut(),
                &IID_ISHELLITEMIMAGEFACTORY,
                &mut factory,
            ) != 0
                || factory.is_null()
            {
                return None;
            }
            let vtbl = *(factory as *mut *const ImageFactoryVtbl);
            let size = u64::from(width) | (u64::from(height) << 32);
            let mut hbitmap: *mut c_void = std::ptr::null_mut();
            let hr = ((*vtbl).get_image)(factory, size, SIIGBF_THUMBNAILONLY, &mut hbitmap);
            ((*vtbl).release)(factory);
            if hr != 0 || hbitmap.is_null() {
                return None;
            }
            let pixels = bitmap_to_rgba(hbitmap);
            DeleteObject(hbitmap);
            pixels
        })();
        if init >= 0 {
            CoUninitialize();
        }
        result
    }
}

unsafe fn bitmap_to_rgba(hbitmap: *mut c_void) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm: BITMAP = std::mem::zeroed();
    if GetObjectW(hbitmap, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void) == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w;
    info.bmiHeader.biHeight = -h; // top-down rows
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let dc = GetDC(std::ptr::null_mut());
    let lines =
        GetDIBits(dc, hbitmap, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut info, DIB_RGB_COLORS);
    ReleaseDC(std::ptr::null_mut(), dc);
    if lines == 0 {
        return None;
    }
    // BGRA to RGBA, opaque.
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
    Some((w as u32, h as u32, buf))
}
