#![windows_subsystem = "windows"]

mod config;
mod renderer;
mod scenes;
mod tray;
mod wallpaper;

use config::{desktop_wallpaper_restore_path, set_autostart, Config, ResMode, SceneId};
use renderer::{is_device_lost, Renderer};
use scenes::FrameInput;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tray::{
    handle_power_broadcast, handle_session_change, is_tray_click, show_popup_menu, TrayState,
    IDM_AUTOPAUSE, IDM_AUTOSTART, IDM_EXIT, IDM_FPS_30, IDM_FPS_60, IDM_MANUAL_PAUSE, IDM_RES_FHD,
    IDM_RES_QHD, IDM_SCENE_PS2_ORIGINAL, IDM_SCENE_PS2_TOWERS, IDM_SCENE_PS2_VIVID,
    IDM_SCENE_PS3_CLASSIC, IDM_SCENE_PS3_OLED, IDM_SCENE_PS4_FLOW, IDM_SCENE_PSP_WAVE,
    WM_OPEN_MENU, WM_TRAY_ICON, WM_WTSSESSION_CHANGE_MSG,
};
use wallpaper::{
    attach_to_workerw, find_workerw, get_primary_display_info, is_desktop_occluded, pin_to_bottom,
    restore_desktop_on_exit, PrimaryDisplayInfo,
};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0,
    WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{
    CancelWaitableTimer, CreateMutexW, CreateWaitableTimerExW, SetWaitableTimer,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    FindWindowW, LoadCursorW, MsgWaitForMultipleObjects, PeekMessageW, PostMessageW,
    PostQuitMessage, RegisterClassExW, TranslateMessage, ASFW_ANY, CW_USEDEFAULT, IDC_ARROW, MSG,
    PM_REMOVE, QS_ALLINPUT, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_POWERBROADCAST, WM_SIZE, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
};

struct AppState {
    cfg: Config,
    preview_mode: bool,
    no_autopause: bool,
    display_info: PrimaryDisplayInfo,
    wall_hwnd: HWND,
    tray_hwnd: HWND,
    workerw: Option<HWND>,
    session_locked: bool,
    display_off: bool,
    occluded: bool,
    needs_resize: bool,
    needs_scene_switch: bool,
    needs_redraw: bool,
    quit: bool,
    tray: Option<TrayState>,
}

impl AppState {
    fn is_paused(&self) -> bool {
        if self.cfg.manual_pause || self.session_locked || self.display_off {
            return true;
        }
        if self.cfg.auto_pause && !self.no_autopause && !self.preview_mode && self.occluded {
            return true;
        }
        false
    }

    fn desired_render_size(&self) -> (u32, u32) {
        let mon_w = (self.display_info.monitor_rect.right - self.display_info.monitor_rect.left)
            .max(1) as u32;
        let mon_h = (self.display_info.monitor_rect.bottom - self.display_info.monitor_rect.top)
            .max(1) as u32;

        if self.preview_mode {
            return (mon_w.min(1280), mon_h.min(720));
        }
        match self.cfg.res_mode {
            ResMode::Fhd => {
                if mon_h <= 1080 {
                    (mon_w, mon_h)
                } else {
                    let target_h = 1080u32;
                    let target_w = ((mon_w as u64 * target_h as u64) / mon_h as u64).max(1) as u32;
                    (target_w, target_h)
                }
            }
            ResMode::Qhd => (mon_w, mon_h),
        }
    }
}

thread_local! {
    static APP_PTR: RefCell<Option<Rc<RefCell<AppState>>>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wall_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            APP_PTR.with(|cell| {
                if let Some(app) = cell.borrow().as_ref() {
                    if let Ok(mut st) = app.try_borrow_mut() {
                        st.needs_resize = true;
                        st.needs_redraw = true;
                    }
                }
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            APP_PTR.with(|cell| {
                if let Some(app) = cell.borrow().as_ref() {
                    if let Ok(mut st) = app.try_borrow_mut() {
                        st.quit = true;
                    }
                }
            });
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn apply_menu_command(st: &mut AppState, cmd: usize) {
    let set_scene = |st: &mut AppState, id: SceneId| {
        if st.cfg.scene != id {
            st.cfg.scene = id;
            st.cfg.save();
            st.needs_scene_switch = true;
            st.needs_redraw = true;
        }
    };

    match cmd {
        IDM_SCENE_PS2_ORIGINAL => set_scene(st, SceneId::Ps2Original),
        IDM_SCENE_PS2_VIVID => set_scene(st, SceneId::Ps2Vivid),
        IDM_SCENE_PS2_TOWERS => set_scene(st, SceneId::Ps2Towers),
        IDM_SCENE_PS3_OLED => set_scene(st, SceneId::Ps3Oled),
        IDM_SCENE_PS3_CLASSIC => set_scene(st, SceneId::Ps3Classic),
        IDM_SCENE_PSP_WAVE => set_scene(st, SceneId::PspWave),
        IDM_SCENE_PS4_FLOW => set_scene(st, SceneId::Ps4Flow),
        IDM_RES_FHD => {
            st.cfg.res_mode = ResMode::Fhd;
            st.cfg.save();
            st.needs_resize = true;
            st.needs_redraw = true;
        }
        IDM_RES_QHD => {
            st.cfg.res_mode = ResMode::Qhd;
            st.cfg.save();
            st.needs_resize = true;
            st.needs_redraw = true;
        }
        IDM_FPS_30 => {
            st.cfg.fps_cap = 30;
            st.cfg.save();
        }
        IDM_FPS_60 => {
            st.cfg.fps_cap = 60;
            st.cfg.save();
        }
        IDM_AUTOPAUSE => {
            st.cfg.auto_pause = !st.cfg.auto_pause;
            st.cfg.save();
            st.needs_redraw = true;
        }
        IDM_MANUAL_PAUSE => {
            st.cfg.manual_pause = !st.cfg.manual_pause;
            st.cfg.save();
            st.needs_redraw = true;
        }
        IDM_AUTOSTART => {
            let enabled = config::is_autostart_enabled();
            set_autostart(!enabled);
        }
        IDM_EXIT => {
            st.quit = true;
            unsafe { PostQuitMessage(0) };
        }
        _ => {}
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if (msg == WM_TRAY_ICON && is_tray_click(lparam)) || msg == WM_OPEN_MENU {
        let mut cfg_opt = None;
        APP_PTR.with(|cell| {
            if let Some(app) = cell.borrow().as_ref() {
                if let Ok(st) = app.try_borrow() {
                    cfg_opt = Some(st.cfg.clone());
                }
            }
        });
        if let Some(cfg_copy) = cfg_opt {
            let cmd = show_popup_menu(hwnd, &cfg_copy);
            if cmd != 0 {
                APP_PTR.with(|cell| {
                    if let Some(app) = cell.borrow().as_ref() {
                        if let Ok(mut st) = app.try_borrow_mut() {
                            apply_menu_command(&mut st, cmd);
                        }
                    }
                });
            }
        }
        return LRESULT(0);
    }

    let mut handled = false;

    APP_PTR.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            if let Ok(mut st) = app.try_borrow_mut() {
                if let Some(ref tray) = st.tray {
                    if msg == tray.taskbar_created_msg && msg != 0 {
                        tray.add_icon();
                        if !st.preview_mode {
                            st.workerw = find_workerw();
                            if let Some(ww) = st.workerw {
                                attach_to_workerw(st.wall_hwnd, ww, st.display_info.monitor_rect);
                            } else {
                                pin_to_bottom(st.wall_hwnd);
                            }
                            st.needs_redraw = true;
                        }
                        handled = true;
                    }
                }

                if !handled {
                    match msg {
                        WM_COMMAND => {
                            let cmd = wparam.0 & 0xFFFF;
                            apply_menu_command(&mut st, cmd);
                            handled = true;
                        }
                        WM_POWERBROADCAST => {
                            if handle_power_broadcast(wparam, lparam, &mut st.display_off) {
                                st.needs_redraw = true;
                            }
                            handled = true;
                        }
                        WM_WTSSESSION_CHANGE_MSG => {
                            if handle_session_change(wparam, &mut st.session_locked) {
                                st.needs_redraw = true;
                            }
                            handled = true;
                        }
                        WM_DISPLAYCHANGE => {
                            st.display_info = get_primary_display_info();
                            if let Some(workerw) = st.workerw {
                                attach_to_workerw(
                                    st.wall_hwnd,
                                    workerw,
                                    st.display_info.monitor_rect,
                                );
                            } else if !st.preview_mode {
                                pin_to_bottom(st.wall_hwnd);
                            }
                            st.needs_resize = true;
                            st.needs_redraw = true;
                            handled = true;
                        }
                        WM_CLOSE | WM_DESTROY => {
                            st.quit = true;
                            PostQuitMessage(0);
                            handled = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

struct CliArgs {
    preview: bool,
    no_autopause: bool,
    scene_override: Option<SceneId>,
    res_override: Option<ResMode>,
    time_override: Option<f64>,
    capture_path: Option<PathBuf>,
}

fn parse_args() -> CliArgs {
    let mut out = CliArgs {
        preview: false,
        no_autopause: false,
        scene_override: None,
        res_override: None,
        time_override: None,
        capture_path: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--preview" => out.preview = true,
            "--no-autopause" => out.no_autopause = true,
            "--qhd" => out.res_override = Some(ResMode::Qhd),
            "--fhd" => out.res_override = Some(ResMode::Fhd),
            "--mode" | "--scene" => {
                if let Some(m) = args.next() {
                    if let Some(id) = SceneId::parse(&m) {
                        out.scene_override = Some(id);
                    }
                }
            }
            "--time" => {
                if let Some(t) = args.next() {
                    if let Ok(v) = t.parse::<f64>() {
                        out.time_override = Some(v);
                    }
                }
            }
            "--capture" => {
                if let Some(p) = args.next() {
                    out.capture_path = Some(PathBuf::from(p));
                }
            }
            _ => {}
        }
    }
    out
}

fn local_clock_info() -> (f64, u32, u32) {
    unsafe {
        let st = GetLocalTime();
        let sec = (st.wHour as f64) * 3600.0
            + (st.wMinute as f64) * 60.0
            + (st.wSecond as f64)
            + (st.wMilliseconds as f64) / 1000.0;
        (sec, st.wMonth as u32, st.wDay as u32)
    }
}

unsafe extern "system" fn capture_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn run_capture(cli: &CliArgs) -> Result<(), String> {
    let hinst = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
    let cls_name = w!("PS2OrbsCaptureWnd");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(capture_wnd_proc),
        hInstance: hinst.into(),
        lpszClassName: cls_name,
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let (w, h) = match cli.res_override {
        Some(ResMode::Qhd) => (2560u32, 1440u32),
        _ => (1920u32, 1080u32),
    };

    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        cls_name,
        w!("PS2 Orbs Capture"),
        WS_POPUP,
        0,
        0,
        w as i32,
        h as i32,
        None,
        None,
        hinst,
        None,
    )
    .map_err(|e| format!("CreateWindowExW capture: {e}"))?;

    let scene = cli.scene_override.unwrap_or(SceneId::Ps2Original);
    let mut renderer = Renderer::new(hwnd, w, h, scene)?;
    let (local_sec, month, day) = local_clock_info();
    let clock_sec = cli.time_override.unwrap_or(local_sec);
    let input = FrameInput {
        clock_sec,
        month,
        day,
        dt: 1.0 / 60.0,
        elapsed: cli.time_override.map(|v| v as f32).unwrap_or(4.5),
        width: w,
        height: h,
        is_capture: true,
    };
    renderer.draw_only(&input);
    let path = cli.capture_path.as_ref().unwrap();
    renderer.capture_bmp(path)?;
    let _ = DestroyWindow(hwnd);
    Ok(())
}

fn qpc_now() -> i64 {
    let mut c = 0i64;
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
    }
    c
}

fn qpc_freq() -> i64 {
    let mut f = 1i64;
    unsafe {
        let _ = QueryPerformanceFrequency(&mut f);
    }
    f.max(1)
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let cli = parse_args();

    if cli.capture_path.is_some() {
        unsafe {
            let _ = run_capture(&cli);
        }
        return;
    }

    let mut mutex_handle = HANDLE::default();
    if !cli.preview {
        unsafe {
            if let Ok(h) = CreateMutexW(None, false, w!("Local\\PS2OrbsWallpaperMutex")) {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    if let Ok(existing) = FindWindowW(w!("PS2OrbsTrayWnd"), PCWSTR::null()) {
                        if !existing.0.is_null() {
                            let _ = AllowSetForegroundWindow(ASFW_ANY);
                            let _ = PostMessageW(existing, WM_OPEN_MENU, WPARAM(0), LPARAM(0));
                        }
                    }
                    let _ = CloseHandle(h);
                    return;
                }
                mutex_handle = h;
            }
        }
    }

    let mut cfg = Config::load();
    if let Some(s) = cli.scene_override {
        cfg.scene = s;
    }
    if let Some(r) = cli.res_override {
        cfg.res_mode = r;
    }

    let saved_wallpaper = if !cli.preview {
        desktop_wallpaper_restore_path()
    } else {
        Vec::new()
    };

    let display_info = get_primary_display_info();
    let workerw = if !cli.preview { find_workerw() } else { None };

    unsafe {
        let hinst = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => return,
        };
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

        let wall_cls = w!("PS2OrbsWallpaperWnd");
        let wc_wall = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wall_wnd_proc),
            hInstance: hinst.into(),
            hCursor: cursor,
            lpszClassName: wall_cls,
            ..Default::default()
        };
        RegisterClassExW(&wc_wall);

        let tray_cls = w!("PS2OrbsTrayWnd");
        let wc_tray = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinst.into(),
            hCursor: cursor,
            lpszClassName: tray_cls,
            ..Default::default()
        };
        RegisterClassExW(&wc_tray);

        let tray_hwnd = match CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            tray_cls,
            w!("PS2 Orbs Tray"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            hinst,
            None,
        ) {
            Ok(h) => h,
            Err(_) => return,
        };

        let (wall_hwnd, win_w, win_h) = if cli.preview {
            let pw = 1280i32;
            let ph = 720i32;
            let h = match CreateWindowExW(
                Default::default(),
                wall_cls,
                w!("PS2 Orbs Preview"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                pw,
                ph,
                None,
                None,
                hinst,
                None,
            ) {
                Ok(h) => h,
                Err(_) => {
                    let _ = DestroyWindow(tray_hwnd);
                    return;
                }
            };
            (h, pw as u32, ph as u32)
        } else {
            let mon = display_info.monitor_rect;
            let mw = (mon.right - mon.left).max(1);
            let mh = (mon.bottom - mon.top).max(1);

            let h = match CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                wall_cls,
                w!("PS2 Orbs Wallpaper"),
                WS_POPUP | WS_VISIBLE,
                mon.left,
                mon.top,
                mw,
                mh,
                HWND::default(),
                None,
                hinst,
                None,
            ) {
                Ok(h) => h,
                Err(_) => {
                    let _ = DestroyWindow(tray_hwnd);
                    return;
                }
            };

            (h, mw as u32, mh as u32)
        };

        let tray = TrayState::new(tray_hwnd);

        let app = Rc::new(RefCell::new(AppState {
            cfg,
            preview_mode: cli.preview,
            no_autopause: cli.no_autopause,
            display_info,
            wall_hwnd,
            tray_hwnd,
            workerw,
            session_locked: false,
            display_off: false,
            occluded: false,
            needs_resize: false,
            needs_scene_switch: false,
            needs_redraw: true,
            quit: false,
            tray: Some(tray),
        }));

        APP_PTR.with(|cell| {
            *cell.borrow_mut() = Some(app.clone());
        });

        let (init_rw, init_rh, init_scene) = {
            let st = app.borrow();
            let (w, h) = if st.preview_mode {
                (win_w, win_h)
            } else {
                st.desired_render_size()
            };
            (w, h, st.cfg.scene)
        };

        let mut renderer = match Renderer::new(wall_hwnd, init_rw, init_rh, init_scene) {
            Ok(r) => r,
            Err(_) => {
                let mut st = app.borrow_mut();
                if let Some(mut t) = st.tray.take() {
                    t.cleanup();
                }
                let _ = DestroyWindow(wall_hwnd);
                let _ = DestroyWindow(tray_hwnd);
                return;
            }
        };

        if !cli.preview {
            if let Some(ww) = workerw {
                attach_to_workerw(wall_hwnd, ww, display_info.monitor_rect);
            } else {
                pin_to_bottom(wall_hwnd);
            }
        }

        let timer = CreateWaitableTimerExW(
            None,
            PCWSTR::null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS.0,
        )
        .unwrap_or_default();

        let freq = qpc_freq();
        let occ_interval = (freq * 300) / 1000;
        let mut last_occ_qpc = qpc_now() - occ_interval;
        let mut last_frame_qpc = qpc_now();
        let mut next_frame_qpc = last_frame_qpc;
        let (init_sec, mut cur_month, mut cur_day) = local_clock_info();
        let mut base_clock_sec: f64 = cli.time_override.unwrap_or(init_sec);
        let mut elapsed_anim_sec: f64 = 0.0;
        let mut total_elapsed: f32 = 0.0;
        let mut was_paused = false;

        let mut msg = MSG::default();
        loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    app.borrow_mut().quit = true;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if app.borrow().quit {
                break;
            }

            let now = qpc_now();
            if now - last_occ_qpc >= occ_interval {
                last_occ_qpc = now;
                let (check_occ, work_rect, wh, th) = {
                    let st = app.borrow();
                    (
                        st.cfg.auto_pause && !st.no_autopause && !st.preview_mode,
                        st.display_info.work_rect,
                        st.wall_hwnd,
                        st.tray_hwnd,
                    )
                };
                if check_occ {
                    let occ = is_desktop_occluded(work_rect, wh, th);
                    let mut st = app.borrow_mut();
                    if st.occluded != occ {
                        st.occluded = occ;
                        if !occ {
                            st.needs_redraw = true;
                        }
                    }
                } else {
                    app.borrow_mut().occluded = false;
                }
            }

            let (
                paused,
                needs_resize,
                needs_scene_switch,
                needs_redraw,
                scene_id,
                fps_cap,
                target_rw,
                target_rh,
            ) = {
                let st = app.borrow();
                let (rw, rh) = if st.preview_mode {
                    let mut rc = windows::Win32::Foundation::RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(
                        st.wall_hwnd,
                        &mut rc,
                    );
                    ((rc.right - rc.left).max(1) as u32, (rc.bottom - rc.top).max(1) as u32)
                } else {
                    st.desired_render_size()
                };
                (
                    st.is_paused(),
                    st.needs_resize,
                    st.needs_scene_switch,
                    st.needs_redraw,
                    st.cfg.scene,
                    st.cfg.fps_cap.clamp(15, 60),
                    rw,
                    rh,
                )
            };

            if needs_scene_switch {
                if renderer.set_scene(scene_id).is_err() {
                    if let Ok(new_r) = Renderer::new(wall_hwnd, target_rw, target_rh, scene_id) {
                        renderer = new_r;
                    }
                }
                total_elapsed = 0.0;
                app.borrow_mut().needs_scene_switch = false;
            }

            if needs_resize {
                if renderer.resize(target_rw, target_rh).is_err() {
                    if let Ok(new_r) = Renderer::new(wall_hwnd, target_rw, target_rh, scene_id) {
                        renderer = new_r;
                    }
                }
                app.borrow_mut().needs_resize = false;
            }

            let frame_ticks = freq / (fps_cap as i64);

            if paused && !needs_redraw {
                was_paused = true;
                if !timer.0.is_null() {
                    let _ = CancelWaitableTimer(timer);
                }
                let (auto_occ_wait, _) = {
                    let st = app.borrow();
                    (
                        st.cfg.auto_pause
                            && !st.no_autopause
                            && !st.preview_mode
                            && !st.cfg.manual_pause
                            && !st.session_locked
                            && !st.display_off,
                        (),
                    )
                };
                let timeout = if auto_occ_wait { 300 } else { INFINITE };
                let _ = MsgWaitForMultipleObjects(None, false, timeout, QS_ALLINPUT);
                last_frame_qpc = qpc_now();
                next_frame_qpc = last_frame_qpc;
                continue;
            }

            let now = qpc_now();
            if now >= next_frame_qpc || needs_redraw {
                if was_paused && !paused {
                    if cli.time_override.is_none() {
                        let (s, m, d) = local_clock_info();
                        base_clock_sec = s;
                        cur_month = m;
                        cur_day = d;
                        elapsed_anim_sec = 0.0;
                    }
                    was_paused = false;
                }
                let dt = ((now - last_frame_qpc) as f64 / freq as f64).clamp(0.0, 0.05) as f32;
                last_frame_qpc = now;
                if !paused {
                    elapsed_anim_sec += dt as f64;
                    total_elapsed += dt;
                }

                let frame_in = FrameInput {
                    clock_sec: base_clock_sec + elapsed_anim_sec,
                    month: cur_month,
                    day: cur_day,
                    dt,
                    elapsed: total_elapsed,
                    width: renderer.width,
                    height: renderer.height,
                    is_capture: false,
                };
                if let Err(hr) = renderer.draw_and_present(&frame_in) {
                    if is_device_lost(hr) {
                        if let Ok(new_r) = Renderer::new(wall_hwnd, target_rw, target_rh, scene_id)
                        {
                            renderer = new_r;
                        }
                    }
                }
                app.borrow_mut().needs_redraw = false;

                next_frame_qpc += frame_ticks;
                if next_frame_qpc <= now {
                    next_frame_qpc = now + frame_ticks;
                }
            }

            let rem_ticks = (next_frame_qpc - qpc_now()).max(0);
            let rem_100ns = -((rem_ticks * 10_000_000) / freq).max(1_000);
            if !timer.0.is_null() {
                let _ = SetWaitableTimer(timer, &rem_100ns, 0, None, None, false);
                let handles = [timer];
                let res = MsgWaitForMultipleObjects(Some(&handles), false, 100, QS_ALLINPUT);
                let _ = res == WAIT_OBJECT_0;
            } else {
                let wait_ms = ((rem_ticks * 1000) / freq).clamp(1, 33) as u32;
                let _ = MsgWaitForMultipleObjects(None, false, wait_ms, QS_ALLINPUT);
            }
        }

        if !timer.0.is_null() {
            let _ = CloseHandle(timer);
        }

        {
            let mut st = app.borrow_mut();
            if let Some(mut t) = st.tray.take() {
                t.cleanup();
            }
        }
        APP_PTR.with(|cell| {
            *cell.borrow_mut() = None;
        });

        drop(renderer);
        let _ = DestroyWindow(wall_hwnd);
        let _ = DestroyWindow(tray_hwnd);

        if !cli.preview {
            restore_desktop_on_exit(workerw, &saved_wallpaper);
        }
        if !mutex_handle.0.is_null() {
            let _ = CloseHandle(mutex_handle);
        }
    }
}
