use crate::config::{is_autostart_enabled, Config, OrbMode, ResMode};
use std::ffi::c_void;
use std::mem::size_of;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, POINT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Power::{
    RegisterPowerSettingNotification, UnregisterPowerSettingNotification, HPOWERNOTIFY,
    POWERBROADCAST_SETTING,
};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuRadioItem, CreateIconIndirect, CreatePopupMenu, DestroyIcon,
    DestroyMenu, GetCursorPos, LoadIconW, PostMessageW, RegisterWindowMessageW,
    SetForegroundWindow, TrackPopupMenu, DEVICE_NOTIFY_WINDOW_HANDLE, HICON, ICONINFO,
    IDI_APPLICATION, MF_BYCOMMAND, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED,
    PBT_POWERSETTINGCHANGE, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

pub const WM_TRAY_ICON: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;
pub const WM_OPEN_MENU: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 2;

pub const IDM_MODE_MENU: usize = 1001;
pub const IDM_MODE_TRAIL: usize = 1002;
pub const IDM_RES_FHD: usize = 1003;
pub const IDM_RES_QHD: usize = 1004;
pub const IDM_FPS_30: usize = 1005;
pub const IDM_FPS_60: usize = 1006;
pub const IDM_AUTOPAUSE: usize = 1007;
pub const IDM_MANUAL_PAUSE: usize = 1008;
pub const IDM_AUTOSTART: usize = 1009;
pub const IDM_EXIT: usize = 1010;

pub const WM_WTSSESSION_CHANGE_MSG: u32 = 0x02B1;

pub struct TrayState {
    pub hwnd: HWND,
    pub hicon: HICON,
    pub taskbar_created_msg: u32,
    pub power_notify: HPOWERNOTIFY,
    pub wts_registered: bool,
}

impl TrayState {
    pub unsafe fn new(hwnd: HWND) -> Self {
        let hicon = create_orb_icon();
        let taskbar_created_msg = RegisterWindowMessageW(w!("TaskbarCreated"));

        let mut state = Self {
            hwnd,
            hicon,
            taskbar_created_msg,
            power_notify: HPOWERNOTIFY::default(),
            wts_registered: false,
        };

        state.add_icon();
        state.wts_registered = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_ok();
        if let Ok(pn) = RegisterPowerSettingNotification(
            HANDLE(hwnd.0),
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        ) {
            state.power_notify = pn;
        }

        state
    }

    pub unsafe fn add_icon(&self) {
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY_ICON,
            hIcon: self.hicon,
            ..Default::default()
        };
        let tip: Vec<u16> = "PS2 Orbs Wallpaper".encode_utf16().collect();
        let copy_len = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }

    pub unsafe fn cleanup(&mut self) {
        let nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        if self.wts_registered {
            let _ = WTSUnRegisterSessionNotification(self.hwnd);
            self.wts_registered = false;
        }
        if self.power_notify.0 != 0 {
            let _ = UnregisterPowerSettingNotification(self.power_notify);
            self.power_notify = HPOWERNOTIFY::default();
        }
        if !self.hicon.0.is_null() {
            let _ = DestroyIcon(self.hicon);
            self.hicon = HICON::default();
        }
    }
}

pub unsafe fn show_popup_menu(hwnd: HWND, cfg: &Config) -> usize {
    let Ok(hmenu) = CreatePopupMenu() else {
        return 0;
    };

    let _ = AppendMenuW(hmenu, MF_STRING, IDM_MODE_MENU, w!("PS2 Original (BIOS Clock)"));
    let _ = AppendMenuW(hmenu, MF_STRING, IDM_MODE_TRAIL, w!("PS2 Vivid Violet Trails"));
    let mode_id = match cfg.mode {
        OrbMode::Menu => IDM_MODE_MENU as u32,
        OrbMode::Trail => IDM_MODE_TRAIL as u32,
    };
    let _ = CheckMenuRadioItem(
        hmenu,
        IDM_MODE_MENU as u32,
        IDM_MODE_TRAIL as u32,
        mode_id,
        MF_BYCOMMAND.0,
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        IDM_RES_FHD,
        w!("FHD (1920×1080) — мінімум VRAM"),
    );
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        IDM_RES_QHD,
        w!("QHD / Нативна (2560×1440)"),
    );
    let res_id = match cfg.res_mode {
        ResMode::Fhd => IDM_RES_FHD as u32,
        ResMode::Qhd => IDM_RES_QHD as u32,
    };
    let _ = CheckMenuRadioItem(
        hmenu,
        IDM_RES_FHD as u32,
        IDM_RES_QHD as u32,
        res_id,
        MF_BYCOMMAND.0,
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    let _ = AppendMenuW(hmenu, MF_STRING, IDM_FPS_30, w!("30 FPS"));
    let _ = AppendMenuW(hmenu, MF_STRING, IDM_FPS_60, w!("60 FPS"));
    let fps_id = if cfg.fps_cap <= 30 {
        IDM_FPS_30 as u32
    } else {
        IDM_FPS_60 as u32
    };
    let _ = CheckMenuRadioItem(
        hmenu,
        IDM_FPS_30 as u32,
        IDM_FPS_60 as u32,
        fps_id,
        MF_BYCOMMAND.0,
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    let _ = AppendMenuW(
        hmenu,
        MF_STRING | if cfg.auto_pause { MF_CHECKED } else { MF_UNCHECKED },
        IDM_AUTOPAUSE,
        w!("Автопауза під вікнами"),
    );
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | if cfg.manual_pause { MF_CHECKED } else { MF_UNCHECKED },
        IDM_MANUAL_PAUSE,
        w!("Пауза"),
    );
    let autostart = is_autostart_enabled();
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | if autostart { MF_CHECKED } else { MF_UNCHECKED },
        IDM_AUTOSTART,
        w!("Автозапуск з Windows"),
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(hmenu, MF_STRING, IDM_EXIT, w!("Вихід"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(hmenu);
    cmd.0 as usize
}

pub fn handle_power_broadcast(wparam: WPARAM, lparam: LPARAM, display_off: &mut bool) -> bool {
    if wparam.0 as u32 == PBT_POWERSETTINGCHANGE && lparam.0 != 0 {
        let setting = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
        if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE && setting.DataLength >= 1 {
            let state = setting.Data[0];
            *display_off = state == 0;
            return true;
        }
    }
    false
}

pub fn handle_session_change(wparam: WPARAM, session_locked: &mut bool) -> bool {
    match wparam.0 as u32 {
        WTS_SESSION_LOCK => {
            *session_locked = true;
            true
        }
        WTS_SESSION_UNLOCK => {
            *session_locked = false;
            true
        }
        _ => false,
    }
}

pub fn is_tray_click(lparam: LPARAM) -> bool {
    let ev = (lparam.0 & 0xFFFF) as u32;
    ev == WM_RBUTTONUP || ev == WM_LBUTTONUP || ev == WM_CONTEXTMENU
}

unsafe fn create_orb_icon() -> HICON {
    const W: i32 = 32;
    const H: i32 = 32;

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: W,
            biHeight: -H,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let dc = CreateCompatibleDC(None);
    let mut bits_ptr: *mut c_void = std::ptr::null_mut();
    let Ok(hbm_color) = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) else {
        let _ = DeleteDC(dc);
        return LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
    };

    let mut mask_bytes = [0u8; ((W * H) / 8) as usize];

    if !bits_ptr.is_null() {
        let pixels = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, (W * H) as usize);
        for y in 0..H {
            for x in 0..W {
                let nx = ((x as f32) + 0.5) / (W as f32) * 2.0 - 1.0;
                let ny = ((y as f32) + 0.5) / (H as f32) * 2.0 - 1.0;
                let r2 = nx * nx + ny * ny;

                if r2 >= 1.0 {
                    pixels[(y * W + x) as usize] = 0;
                    let idx = (y * 4 + (x >> 3)) as usize;
                    mask_bytes[idx] |= 0x80 >> (x & 7);
                    continue;
                }

                // High-contrast electric cyan halo (#1CA8FF) + pure white core (#FFFFFF)
                let d = (1.0 - r2).max(0.0);
                let halo = d.powf(1.85);
                let core = (-r2 * 10.5).exp();

                let alpha = (halo.max(core) * 1.15).clamp(0.0, 1.0);
                let a = (alpha * 255.0) as u32;
                if a < 8 {
                    pixels[(y * W + x) as usize] = 0;
                    let idx = (y * 4 + (x >> 3)) as usize;
                    mask_bytes[idx] |= 0x80 >> (x & 7);
                } else {
                    let r = ((0.11 * halo + 1.00 * core).clamp(0.0, 1.0) * 255.0) as u32;
                    let g = ((0.66 * halo + 1.00 * core).clamp(0.0, 1.0) * 255.0) as u32;
                    let b = ((1.00 * halo + 1.00 * core).clamp(0.0, 1.0) * 255.0) as u32;
                    pixels[(y * W + x) as usize] = (a << 24) | (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    let hbm_mask: HBITMAP = CreateBitmap(W, H, 1, 1, Some(mask_bytes.as_ptr() as *const c_void));

    let ii = ICONINFO {
        fIcon: TRUE,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm_color,
    };

    let hicon = CreateIconIndirect(&ii).unwrap_or_default();
    let _ = DeleteObject(hbm_color);
    let _ = DeleteObject(hbm_mask);
    let _ = DeleteDC(dc);

    if hicon.0.is_null() {
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    } else {
        hicon
    }
}
