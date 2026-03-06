#![cfg(target_os = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]

use anyhow::{Context, Result, anyhow, bail};
use std::mem::size_of;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use win_instant_replay::config::{AppConfig, HotKeySpec, KeyCode, Modifiers, load_or_create};
use win_instant_replay::ffmpeg::{CaptureSupervisor, save_replay};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, IDC_ARROW, IDI_APPLICATION, LoadCursorW,
    LoadIconW, MENU_ITEM_FLAGS, MSG, PostQuitMessage, RegisterClassW, SW_HIDE, SetForegroundWindow,
    ShowWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TRACK_POPUP_MENU_FLAGS, TrackPopupMenu,
    TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY,
    WM_LBUTTONDBLCLK, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::PCWSTR;

const WINDOW_CLASS: &str = "WinInstantReplayHiddenWindow";
const TRAY_UID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const MENU_OPEN_OUTPUT: usize = 1001;
const MENU_QUIT: usize = 1002;

static APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

struct AppState {
    config: Arc<AppConfig>,
    capture: Mutex<Option<CaptureSupervisor>>,
}

pub fn run() -> Result<()> {
    let config = Arc::new(load_or_create()?);
    let capture = CaptureSupervisor::start(config.clone());
    let state = Arc::new(AppState {
        config,
        capture: Mutex::new(Some(capture)),
    });

    APP_STATE
        .set(state)
        .map_err(|_| anyhow!("application state already initialized"))?;

    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = wide_null(WINDOW_CLASS);
        let window_title = wide_null("Win Instant Replay");

        let window_class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW).context("loading cursor")?,
            hIcon: LoadIconW(None, IDI_APPLICATION).context("loading tray icon")?,
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            lpfnWndProc: Some(window_proc),
            ..Default::default()
        };

        let atom = RegisterClassW(&window_class);
        if atom == 0 {
            bail!("RegisterClassW failed");
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        let _ = ShowWindow(hwnd, SW_HIDE);
        add_tray_icon(hwnd)?;
        register_hotkeys(hwnd, APP_STATE.get().unwrap().config.as_ref())?;
        show_notification(
            hwnd,
            "Win Instant Replay",
            "Background capture is running.",
            false,
        );

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_HOTKEY => {
            if let Some(state) = APP_STATE.get() {
                let hotkey_id = wparam.0 as i32;
                if let Some(binding) = state
                    .config
                    .hotkeys
                    .iter()
                    .find(|binding| binding.id == hotkey_id)
                {
                    let config = state.config.clone();
                    let duration = binding.duration_seconds;
                    // HWND is not Send, so the worker thread carries the raw handle value and
                    // reconstructs it only to trigger a tray balloon on completion.
                    let notify_hwnd = hwnd.0 as isize;
                    thread::spawn(move || match save_replay(config.as_ref(), duration) {
                        Ok(path) => {
                            let body = format!("Saved {}s replay to {}", duration, path.display());
                            let hwnd = HWND(notify_hwnd as *mut _);
                            unsafe { show_notification(hwnd, "Replay saved", &body, false) };
                        }
                        Err(error) => {
                            let body = format!("Could not save {}s replay: {error}", duration);
                            let hwnd = HWND(notify_hwnd as *mut _);
                            unsafe { show_notification(hwnd, "Replay failed", &body, true) };
                        }
                    });
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match loword(wparam.0) as usize {
                MENU_OPEN_OUTPUT => {
                    if let Some(state) = APP_STATE.get() {
                        if let Err(error) = open_output_folder(&state.config.output_dir) {
                            let body = format!("Failed to open output folder: {error}");
                            show_notification(hwnd, "Open folder failed", &body, true);
                        }
                    }
                }
                MENU_QUIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            match lparam.0 as u32 {
                WM_CONTEXTMENU | WM_RBUTTONUP => {
                    let _ = show_tray_menu(hwnd);
                }
                WM_LBUTTONDBLCLK => {
                    if let Some(state) = APP_STATE.get() {
                        if let Err(error) = open_output_folder(&state.config.output_dir) {
                            let body = format!("Failed to open output folder: {error}");
                            show_notification(hwnd, "Open folder failed", &body, true);
                        }
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(state) = APP_STATE.get() {
                unregister_hotkeys(hwnd, state.config.as_ref());
            }
            remove_tray_icon(hwnd);
            if let Some(state) = APP_STATE.get() {
                if let Ok(mut capture_guard) = state.capture.lock() {
                    if let Some(capture) = capture_guard.take() {
                        capture.stop();
                    }
                }
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn register_hotkeys(hwnd: HWND, config: &AppConfig) -> Result<()> {
    for binding in &config.hotkeys {
        let (modifiers, key) = to_windows_hotkey(binding.parsed);
        RegisterHotKey(Some(hwnd), binding.id, modifiers, key).with_context(|| {
            format!(
                "registering hotkey {} for {}s replay",
                binding.combo, binding.duration_seconds
            )
        })?;
    }
    Ok(())
}

unsafe fn unregister_hotkeys(hwnd: HWND, config: &AppConfig) {
    for binding in &config.hotkeys {
        let _ = UnregisterHotKey(Some(hwnd), binding.id);
    }
}

unsafe fn add_tray_icon(hwnd: HWND) -> Result<()> {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_MESSAGE | NIF_TIP | NIF_ICON,
        uCallbackMessage: WM_TRAYICON,
        hIcon: LoadIconW(None, IDI_APPLICATION).context("loading tray icon")?,
        ..Default::default()
    };
    write_wide_buffer(&mut data.szTip, "Win Instant Replay");

    if !Shell_NotifyIconW(NIM_ADD, &mut data).as_bool() {
        bail!("Shell_NotifyIconW(NIM_ADD) failed");
    }

    Ok(())
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &mut data);
}

unsafe fn show_notification(hwnd: HWND, title: &str, body: &str, is_error: bool) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_INFO,
        dwInfoFlags: if is_error { NIIF_ERROR } else { NIIF_INFO },
        ..Default::default()
    };
    write_wide_buffer(&mut data.szInfoTitle, title);
    write_wide_buffer(&mut data.szInfo, body);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &mut data);
}

unsafe fn show_tray_menu(hwnd: HWND) -> Result<()> {
    let menu = CreatePopupMenu().context("CreatePopupMenu failed")?;
    let open_text = wide_null("Open Output Folder");
    let quit_text = wide_null("Quit");

    AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        MENU_OPEN_OUTPUT,
        PCWSTR(open_text.as_ptr()),
    )?;
    AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        MENU_QUIT,
        PCWSTR(quit_text.as_ptr()),
    )?;

    let mut point = POINT::default();
    GetCursorPos(&mut point)?;
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(
        menu,
        TRACK_POPUP_MENU_FLAGS(TPM_LEFTALIGN.0 | TPM_BOTTOMALIGN.0),
        point.x,
        point.y,
        Some(0),
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    Ok(())
}

fn open_output_folder(path: &Path) -> Result<()> {
    Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .with_context(|| format!("launching explorer for {}", path.display()))?;
    Ok(())
}

fn write_wide_buffer<const N: usize>(buffer: &mut [u16; N], value: &str) {
    buffer.fill(0);
    for (slot, ch) in buffer.iter_mut().zip(value.encode_utf16()) {
        *slot = ch;
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn loword(value: usize) -> u16 {
    (value & 0xffff) as u16
}

fn to_windows_hotkey(spec: HotKeySpec) -> (HOT_KEY_MODIFIERS, u32) {
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    if spec.modifiers.contains(Modifiers::ALT) {
        modifiers |= MOD_ALT;
    }
    if spec.modifiers.contains(Modifiers::CONTROL) {
        modifiers |= MOD_CONTROL;
    }
    if spec.modifiers.contains(Modifiers::SHIFT) {
        modifiers |= MOD_SHIFT;
    }
    if spec.modifiers.contains(Modifiers::WIN) {
        modifiers |= MOD_WIN;
    }

    let key = match spec.key {
        KeyCode::Digit(digit) => 0x30 + digit as u32,
        KeyCode::Letter(ch) => ch as u32,
        KeyCode::Function(number) => 0x70 + (number as u32 - 1),
    };

    (modifiers, key)
}
