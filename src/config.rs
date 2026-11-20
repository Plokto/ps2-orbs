use std::fs;
use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, REG_VALUE_TYPE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbMode {
    Menu,
    Trail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResMode {
    Fhd,
    Qhd,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: OrbMode,
    pub res_mode: ResMode,
    pub fps_cap: u32,
    pub auto_pause: bool,
    pub manual_pause: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: OrbMode::Menu,
            res_mode: ResMode::Fhd,
            fps_cap: 60,
            auto_pause: true,
            manual_pause: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Self::default();
        if let Some(path) = Self::find_existing_config_path() {
            if let Ok(text) = fs::read_to_string(path) {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let k = k.trim();
                        let v = v.trim();
                        match k {
                            "mode" => {
                                cfg.mode = match v {
                                    "trail" => OrbMode::Trail,
                                    _ => OrbMode::Menu,
                                };
                            }
                            "res_mode" => {
                                cfg.res_mode = match v {
                                    "qhd" => ResMode::Qhd,
                                    _ => ResMode::Fhd,
                                };
                            }
                            "fps_cap" => {
                                if let Ok(fps) = v.parse::<u32>() {
                                    cfg.fps_cap = if fps <= 30 { 30 } else { 60 };
                                }
                            }
                            "auto_pause" => cfg.auto_pause = v == "1" || v.eq_ignore_ascii_case("true"),
                            "manual_pause" => {
                                cfg.manual_pause = v == "1" || v.eq_ignore_ascii_case("true")
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        cfg
    }

    pub fn save(&self) {
        let mode_str = match self.mode {
            OrbMode::Menu => "menu",
            OrbMode::Trail => "trail",
        };
        let res_str = match self.res_mode {
            ResMode::Fhd => "fhd",
            ResMode::Qhd => "qhd",
        };
        let content = format!(
            "mode={}\nres_mode={}\nfps_cap={}\nauto_pause={}\nmanual_pause={}\n",
            mode_str,
            res_str,
            self.fps_cap,
            if self.auto_pause { 1 } else { 0 },
            if self.manual_pause { 1 } else { 0 },
        );

        if let Some(exe_ini) = Self::exe_config_path() {
            if fs::write(&exe_ini, &content).is_ok() {
                return;
            }
        }
        if let Some(app_ini) = Self::appdata_config_path() {
            if let Some(parent) = app_ini.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&app_ini, &content);
        }
    }

    fn find_existing_config_path() -> Option<PathBuf> {
        if let Some(p) = Self::exe_config_path() {
            if p.exists() {
                return Some(p);
            }
        }
        if let Some(p) = Self::appdata_config_path() {
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    fn exe_config_path() -> Option<PathBuf> {
        let exe = get_exe_path()?;
        let dir = exe.parent()?;
        Some(dir.join("ps2-orbs.ini"))
    }

    fn appdata_config_path() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(PathBuf::from(base).join("PS2OrbsWallpaper").join("ps2-orbs.ini"))
    }
}

pub fn get_exe_path() -> Option<PathBuf> {
    let mut buf = vec![0u16; MAX_PATH as usize * 2];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..len]);
    Some(PathBuf::from(s))
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "PS2OrbsWallpaper";

pub fn is_autostart_enabled() -> bool {
    let subkey = to_wide_null(RUN_SUBKEY);
    let valname = to_wide_null(RUN_VALUE);
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let mut typ = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        let res = RegQueryValueExW(
            hkey,
            PCWSTR(valname.as_ptr()),
            None,
            Some(&mut typ),
            None,
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);
        res == ERROR_SUCCESS && size > 0
    }
}

pub fn set_autostart(enabled: bool) {
    let subkey = to_wide_null(RUN_SUBKEY);
    let valname = to_wide_null(RUN_VALUE);
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_WRITE,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return;
        }
        if enabled {
            if let Some(exe) = get_exe_path() {
                let cmd = format!("\"{}\"", exe.to_string_lossy());
                let wide = to_wide_null(&cmd);
                let bytes = std::slice::from_raw_parts(
                    wide.as_ptr() as *const u8,
                    wide.len() * std::mem::size_of::<u16>(),
                );
                let _ = RegSetValueExW(hkey, PCWSTR(valname.as_ptr()), 0, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(valname.as_ptr()));
        }
        let _ = RegCloseKey(hkey);
    }
}

pub fn desktop_wallpaper_restore_path() -> Vec<u16> {
    let mut buf = vec![0u16; MAX_PATH as usize * 2];
    let ok = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows::Win32::UI::WindowsAndMessaging::SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        return vec![0u16];
    }
    let _ = PWSTR(buf.as_mut_ptr());
    buf
}
