use std::ffi::c_void;
use std::mem::size_of;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, RedrawWindow, ScreenToClient, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetLayeredWindowAttributes,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible, SendMessageTimeoutW, SetParent,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, GWL_EXSTYLE, GWL_STYLE,
    HWND_BOTTOM, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, SMTO_NORMAL, SPIF_SENDCHANGE,
    SPI_SETDESKWALLPAPER, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_VISIBLE,
};

#[derive(Clone, Copy, Debug)]
pub struct PrimaryDisplayInfo {
    pub monitor_rect: RECT,
    pub work_rect: RECT,
}

pub fn get_primary_display_info() -> PrimaryDisplayInfo {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            PrimaryDisplayInfo {
                monitor_rect: mi.rcMonitor,
                work_rect: mi.rcWork,
            }
        } else {
            let r = RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            };
            PrimaryDisplayInfo {
                monitor_rect: r,
                work_rect: r,
            }
        }
    }
}

unsafe extern "system" fn enum_find_workerw(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut HWND);
    if let Ok(shell_view) = FindWindowExW(hwnd, HWND::default(), w!("SHELLDLL_DefView"), PCWSTR::null()) {
        if !shell_view.0.is_null() {
            if let Ok(worker) = FindWindowExW(HWND::default(), hwnd, w!("WorkerW"), PCWSTR::null()) {
                if !worker.0.is_null() {
                    *out = worker;
                    return BOOL(0);
                }
            }
        }
    }
    TRUE
}

pub fn find_workerw() -> Option<HWND> {
    unsafe {
        let Ok(progman) = FindWindowW(w!("Progman"), PCWSTR::null()) else {
            return None;
        };
        if progman.0.is_null() {
            return None;
        }

        let mut _res: usize = 0;
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0x0D),
            LPARAM(0x01),
            SMTO_NORMAL,
            1000,
            Some(&mut _res),
        );

        let mut workerw = HWND::default();
        let _ = EnumWindows(
            Some(enum_find_workerw),
            LPARAM(&mut workerw as *mut HWND as isize),
        );

        if workerw.0.is_null() {
            let _ = SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut _res),
            );
            let _ = EnumWindows(
                Some(enum_find_workerw),
                LPARAM(&mut workerw as *mut HWND as isize),
            );
        }

        if workerw.0.is_null() {
            if let Ok(child_w) =
                FindWindowExW(progman, HWND::default(), w!("WorkerW"), PCWSTR::null())
            {
                if !child_w.0.is_null() {
                    return Some(child_w);
                }
            }
            return Some(progman);
        }

        Some(workerw)
    }
}

pub fn attach_to_workerw(child_hwnd: HWND, host_hwnd: HWND, monitor_rect: RECT) -> (u32, u32) {
    unsafe {
        let _ = ShowWindow(host_hwnd, SW_SHOW);
        let _ = SetParent(child_hwnd, host_hwnd);
        let _ = SetWindowLongPtrW(
            child_hwnd,
            GWL_STYLE,
            (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN).0 as isize,
        );

        let mut top_left = POINT {
            x: monitor_rect.left,
            y: monitor_rect.top,
        };
        let _ = ScreenToClient(host_hwnd, &mut top_left);
        let width = (monitor_rect.right - monitor_rect.left).max(1);
        let height = (monitor_rect.bottom - monitor_rect.top).max(1);

        let _ = SetWindowPos(
            child_hwnd,
            HWND_BOTTOM,
            top_left.x,
            top_left.y,
            width,
            height,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        (width as u32, height as u32)
    }
}

pub fn pin_to_bottom(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn restore_desktop_on_exit(workerw: Option<HWND>, saved_wallpaper: &[u16]) {
    unsafe {
        if saved_wallpaper.first().copied().unwrap_or(0) != 0 {
            let _ = SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                Some(saved_wallpaper.as_ptr() as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(SPIF_SENDCHANGE.0),
            );
        } else if let Some(w) = workerw {
            let _ = InvalidateRect(w, None, TRUE);
            let _ = RedrawWindow(
                w,
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
        }
    }
}

struct OcclusionCtx {
    work_rect: RECT,
    work_area: i64,
    ignore_a: HWND,
    ignore_b: HWND,
    occluded: bool,
}

unsafe extern "system" fn enum_occlusion_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut OcclusionCtx);
    if hwnd == ctx.ignore_a || hwnd == ctx.ignore_b {
        return TRUE;
    }
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return TRUE;
    }

    let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0
        || (ex_style & WS_EX_NOACTIVATE.0) != 0
        || (ex_style & WS_EX_TRANSPARENT.0) != 0
    {
        return TRUE;
    }

    if (ex_style & WS_EX_LAYERED.0) != 0 {
        let mut alpha = 255u8;
        let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
        if GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags)).is_ok()
            && (flags.0 & LWA_ALPHA.0) != 0
            && alpha < 245
        {
            return TRUE;
        }
    }

    let mut cloaked: u32 = 0;
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut c_void,
        size_of::<u32>() as u32,
    )
    .is_ok()
        && cloaked != 0
    {
        return TRUE;
    }

    let mut cls_buf = [0u16; 64];
    let cls_len = GetClassNameW(hwnd, &mut cls_buf) as usize;
    if cls_len > 0 {
        let cls = String::from_utf16_lossy(&cls_buf[..cls_len]);
        if matches!(
            cls.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return TRUE;
        }
    }

    let mut rect = RECT::default();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut rect as *mut RECT as *mut c_void,
        size_of::<RECT>() as u32,
    )
    .is_err()
        && GetWindowRect(hwnd, &mut rect).is_err()
    {
        return TRUE;
    }

    let ix0 = rect.left.max(ctx.work_rect.left);
    let iy0 = rect.top.max(ctx.work_rect.top);
    let ix1 = rect.right.min(ctx.work_rect.right);
    let iy1 = rect.bottom.min(ctx.work_rect.bottom);

    if ix1 > ix0 && iy1 > iy0 {
        let inter_area = (ix1 - ix0) as i64 * (iy1 - iy0) as i64;
        if inter_area * 100 >= ctx.work_area * 98 {
            ctx.occluded = true;
            return BOOL(0);
        }
    }

    TRUE
}

pub fn is_desktop_occluded(work_rect: RECT, wallpaper_hwnd: HWND, tray_hwnd: HWND) -> bool {
    let area = (work_rect.right - work_rect.left).max(1) as i64
        * (work_rect.bottom - work_rect.top).max(1) as i64;
    let mut ctx = OcclusionCtx {
        work_rect,
        work_area: area,
        ignore_a: wallpaper_hwnd,
        ignore_b: tray_hwnd,
        occluded: false,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_occlusion_cb),
            LPARAM(&mut ctx as *mut OcclusionCtx as isize),
        );
    }
    ctx.occluded
}
