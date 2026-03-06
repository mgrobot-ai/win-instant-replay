#![cfg(target_os = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]

use anyhow::{Context, Result, anyhow, bail};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use win_instant_replay::config::{
    AppConfig, AppPaths, FileConfig, HotKeySpec, HotkeyEntry, KeyCode, Modifiers,
    default_hotkey_combination, ensure_runtime_dirs, load_or_create_file_config,
    load_or_create_with_paths, save_file_config,
};
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
    AppendMenuW, BM_GETCHECK, BM_SETCHECK, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetWindowTextLengthW,
    GetWindowTextW, IDC_ARROW, IDI_APPLICATION, IsWindow, LoadCursorW, LoadIconW, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MENU_ITEM_FLAGS, MSG, MessageBoxW, PostQuitMessage, RegisterClassW,
    SW_HIDE, SW_SHOW, SendMessageW, SetForegroundWindow, ShowWindow, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TRACK_POPUP_MENU_FLAGS, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CREATE, WM_DESTROY, WM_HOTKEY,
    WM_LBUTTONDBLCLK, WM_RBUTTONUP, WNDCLASSW, WS_BORDER, WS_CHILD, WS_OVERLAPPEDWINDOW,
    WS_TABSTOP, WS_VISIBLE,
};
use windows::core::PCWSTR;

const WINDOW_CLASS: &str = "WinInstantReplayHiddenWindow";
const SETTINGS_WINDOW_CLASS: &str = "WinInstantReplaySettingsWindow";
const TRAY_UID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const MENU_SETTINGS: usize = 1000;
const MENU_OPEN_OUTPUT: usize = 1001;
const MENU_QUIT: usize = 1002;

static APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

struct AppState {
    paths: AppPaths,
    main_window: Mutex<Option<isize>>,
    settings_window: Mutex<Option<SettingsWindowState>>,
    runtime: Mutex<RuntimeState>,
}

struct RuntimeState {
    config: AppConfig,
    capture: Option<CaptureSupervisor>,
}

#[derive(Clone, Copy)]
struct HotkeyFieldHandle {
    duration_seconds: u32,
    handle: isize,
}

#[derive(Clone)]
struct SettingsWindowState {
    window: isize,
    output_dir: isize,
    ffmpeg_path: isize,
    max_replay_seconds: isize,
    segment_seconds: isize,
    system_audio_enabled: isize,
    system_audio_backend: isize,
    system_audio_device: isize,
    microphone_enabled: isize,
    microphone_backend: isize,
    microphone_device: isize,
    audio_sample_rate: isize,
    audio_channels: isize,
    audio_bitrate: isize,
    hotkeys: [HotkeyFieldHandle; 5],
    save_button: isize,
    cancel_button: isize,
}

pub fn run() -> Result<()> {
    let (paths, config) = load_or_create_with_paths()?;
    let capture = CaptureSupervisor::start(Arc::new(config.clone()));
    let state = Arc::new(AppState {
        paths,
        main_window: Mutex::new(None),
        settings_window: Mutex::new(None),
        runtime: Mutex::new(RuntimeState {
            config,
            capture: Some(capture),
        }),
    });

    APP_STATE
        .set(state)
        .map_err(|_| anyhow!("application state already initialized"))?;

    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        register_window_class(hinstance.into(), WINDOW_CLASS, Some(window_proc))?;
        register_window_class(
            hinstance.into(),
            SETTINGS_WINDOW_CLASS,
            Some(settings_window_proc),
        )?;

        let class_name = wide_null(WINDOW_CLASS);
        let window_title = wide_null("Win Instant Replay");
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

        if let Some(state) = APP_STATE.get() {
            *state.main_window.lock().unwrap() = Some(hwnd.0 as isize);
        }

        let _ = ShowWindow(hwnd, SW_HIDE);
        add_tray_icon(hwnd)?;
        register_current_hotkeys(hwnd)?;
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
            if let Some((config, duration)) = hotkey_request(wparam.0 as i32) {
                let notify_hwnd = hwnd.0 as isize;
                thread::spawn(move || match save_replay(&config, duration) {
                    Ok(path) => {
                        let body = format!("Saved {}s replay to {}", duration, path.display());
                        let hwnd = hwnd_from_isize(notify_hwnd);
                        unsafe { show_notification(hwnd, "Replay saved", &body, false) };
                    }
                    Err(error) => {
                        let body = format!("Could not save {}s replay: {error}", duration);
                        let hwnd = hwnd_from_isize(notify_hwnd);
                        unsafe { show_notification(hwnd, "Replay failed", &body, true) };
                    }
                });
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match loword(wparam.0) as usize {
                MENU_SETTINGS => {
                    if let Err(error) = open_settings_window() {
                        let body = format!("Failed to open settings: {error}");
                        show_notification(hwnd, "Settings failed", &body, true);
                    }
                }
                MENU_OPEN_OUTPUT => {
                    if let Err(error) = open_output_folder(&current_output_dir()) {
                        let body = format!("Failed to open output folder: {error}");
                        show_notification(hwnd, "Open folder failed", &body, true);
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
                    if let Err(error) = open_settings_window() {
                        let body = format!("Failed to open settings: {error}");
                        show_notification(hwnd, "Settings failed", &body, true);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(state) = APP_STATE.get() {
                let config = state.runtime.lock().unwrap().config.clone();
                unregister_hotkeys(hwnd, &config);

                let settings_hwnd = state
                    .settings_window
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|window| window.window);
                if let Some(settings_hwnd) = settings_hwnd {
                    let _ = DestroyWindow(hwnd_from_isize(settings_hwnd));
                }

                remove_tray_icon(hwnd);
                if let Some(capture) = state.runtime.lock().unwrap().capture.take() {
                    capture.stop();
                }
                *state.main_window.lock().unwrap() = None;
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn settings_window_proc(
    hwnd: HWND,
    message: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => match create_settings_controls(hwnd) {
            Ok(window_state) => {
                if let Some(state) = APP_STATE.get() {
                    *state.settings_window.lock().unwrap() = Some(window_state);
                }
                LRESULT(0)
            }
            Err(error) => {
                show_modal_message_box(hwnd, "Settings window", &format!("{error:#}"), true);
                LRESULT(-1)
            }
        },
        WM_COMMAND => {
            if let Some(window_state) = current_settings_window() {
                let source_hwnd = if lparam.0 == 0 {
                    None
                } else {
                    Some(hwnd_from_isize(lparam.0 as isize))
                };

                if let Some(source_hwnd) = source_hwnd {
                    if source_hwnd == hwnd_from_isize(window_state.save_button) {
                        match save_settings_from_window(hwnd) {
                            Ok(()) => {
                                let _ = DestroyWindow(hwnd);
                            }
                            Err(error) => {
                                show_modal_message_box(
                                    hwnd,
                                    "Could not save settings",
                                    &format!("{error:#}"),
                                    true,
                                );
                            }
                        }
                        return LRESULT(0);
                    }
                    if source_hwnd == hwnd_from_isize(window_state.cancel_button) {
                        let _ = DestroyWindow(hwnd);
                        return LRESULT(0);
                    }
                }
            }
            DefWindowProcW(hwnd, message, _wparam, lparam)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(state) = APP_STATE.get() {
                *state.settings_window.lock().unwrap() = None;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, _wparam, lparam),
    }
}

unsafe fn register_window_class(
    hinstance: windows::Win32::Foundation::HINSTANCE,
    class_name: &str,
    proc: windows::Win32::UI::WindowsAndMessaging::WNDPROC,
) -> Result<()> {
    let class_name = wide_null(class_name);
    let window_class = WNDCLASSW {
        hCursor: LoadCursorW(None, IDC_ARROW).context("loading cursor")?,
        hIcon: LoadIconW(None, IDI_APPLICATION).context("loading window icon")?,
        hInstance: hinstance,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        lpfnWndProc: proc,
        ..Default::default()
    };

    let atom = RegisterClassW(&window_class);
    if atom == 0 {
        bail!("RegisterClassW failed")
    }

    Ok(())
}

fn hotkey_request(hotkey_id: i32) -> Option<(AppConfig, u32)> {
    let state = APP_STATE.get()?;
    let runtime = state.runtime.lock().ok()?;
    let binding = runtime
        .config
        .hotkeys
        .iter()
        .find(|binding| binding.id == hotkey_id)?;
    Some((runtime.config.clone(), binding.duration_seconds))
}

fn current_output_dir() -> PathBuf {
    APP_STATE
        .get()
        .and_then(|state| {
            state
                .runtime
                .lock()
                .ok()
                .map(|runtime| runtime.config.output_dir.clone())
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

fn current_settings_window() -> Option<SettingsWindowState> {
    APP_STATE.get().and_then(|state| {
        state
            .settings_window
            .lock()
            .ok()
            .and_then(|window| window.clone())
    })
}

unsafe fn register_current_hotkeys(hwnd: HWND) -> Result<()> {
    let state = APP_STATE
        .get()
        .ok_or_else(|| anyhow!("application state not initialized"))?;
    let config = state.runtime.lock().unwrap().config.clone();
    register_hotkeys(hwnd, &config)
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
    let settings_text = wide_null("Settings");
    let open_text = wide_null("Open Output Folder");
    let quit_text = wide_null("Quit");

    AppendMenuW(
        menu,
        MENU_ITEM_FLAGS(0),
        MENU_SETTINGS,
        PCWSTR(settings_text.as_ptr()),
    )?;
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

unsafe fn open_settings_window() -> Result<()> {
    if let Some(existing) = current_settings_window() {
        let existing_hwnd = hwnd_from_isize(existing.window);
        if IsWindow(Some(existing_hwnd)).as_bool() {
            let _ = ShowWindow(existing_hwnd, SW_SHOW);
            let _ = SetForegroundWindow(existing_hwnd);
            return Ok(());
        }
    }

    let hinstance = GetModuleHandleW(None)?;
    let class_name = wide_null(SETTINGS_WINDOW_CLASS);
    let window_title = wide_null("Win Instant Replay Settings");
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(class_name.as_ptr()),
        PCWSTR(window_title.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        120,
        120,
        780,
        640,
        None,
        None,
        Some(hinstance.into()),
        None,
    )?;

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

unsafe fn create_settings_controls(hwnd: HWND) -> Result<SettingsWindowState> {
    let current = APP_STATE
        .get()
        .ok_or_else(|| anyhow!("application state not initialized"))?;
    let file_config = load_or_create_file_config(&current.paths)?;

    create_label(
        hwnd,
        "Leave ffmpeg blank to use ffmpeg.exe on PATH. Leave output blank to use the default Videos folder.",
        20,
        16,
        730,
        20,
    )?;
    create_label(
        hwnd,
        "Audio devices are plain text fields. Use ffmpeg device names if you are not using the defaults.",
        20,
        38,
        730,
        20,
    )?;

    create_label(hwnd, "Output directory", 20, 72, 130, 20)?;
    let output_dir = create_edit(
        hwnd,
        file_config
            .output_dir
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .as_deref()
            .unwrap_or(""),
        180,
        68,
        230,
        24,
    )?;

    create_label(hwnd, "ffmpeg path", 20, 110, 130, 20)?;
    let ffmpeg_path = create_edit(
        hwnd,
        file_config
            .ffmpeg_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .as_deref()
            .unwrap_or(""),
        180,
        106,
        230,
        24,
    )?;

    create_label(hwnd, "Replay buffer seconds", 20, 148, 150, 20)?;
    let max_replay_seconds = create_edit(
        hwnd,
        &file_config.max_replay_seconds.to_string(),
        180,
        144,
        120,
        24,
    )?;

    create_label(hwnd, "Segment length seconds", 20, 186, 150, 20)?;
    let segment_seconds = create_edit(
        hwnd,
        &file_config.segment_seconds.to_string(),
        180,
        182,
        120,
        24,
    )?;

    let system_audio_enabled = create_checkbox(
        hwnd,
        "Enable system audio",
        file_config.system_audio_enabled,
        20,
        228,
        200,
        24,
    )?;

    create_label(hwnd, "System backend", 20, 266, 130, 20)?;
    let system_audio_backend =
        create_edit(hwnd, &file_config.system_audio_backend, 180, 262, 120, 24)?;

    create_label(hwnd, "System device", 20, 304, 130, 20)?;
    let system_audio_device =
        create_edit(hwnd, &file_config.system_audio_device, 180, 300, 230, 24)?;

    let microphone_enabled = create_checkbox(
        hwnd,
        "Enable microphone",
        file_config.microphone_enabled,
        20,
        342,
        200,
        24,
    )?;

    create_label(hwnd, "Mic backend", 20, 380, 130, 20)?;
    let microphone_backend = create_edit(hwnd, &file_config.microphone_backend, 180, 376, 120, 24)?;

    create_label(hwnd, "Mic device", 20, 418, 130, 20)?;
    let microphone_device = create_edit(hwnd, &file_config.microphone_device, 180, 414, 230, 24)?;

    create_label(hwnd, "Audio sample rate", 20, 456, 130, 20)?;
    let audio_sample_rate = create_edit(
        hwnd,
        &file_config.audio_sample_rate.to_string(),
        180,
        452,
        120,
        24,
    )?;

    create_label(hwnd, "Audio channels", 20, 494, 130, 20)?;
    let audio_channels = create_edit(
        hwnd,
        &file_config.audio_channels.to_string(),
        180,
        490,
        120,
        24,
    )?;

    create_label(hwnd, "Audio bitrate", 20, 532, 130, 20)?;
    let audio_bitrate = create_edit(hwnd, &file_config.audio_bitrate, 180, 528, 120, 24)?;

    create_label(hwnd, "Global hotkeys", 440, 72, 150, 20)?;
    create_label(
        hwnd,
        "Backends: use 'wasapi' or 'dshow'.\r\nSystem audio usually works with wasapi + default.\r\nDirectShow devices need exact ffmpeg device names.",
        440,
        266,
        300,
        54,
    )?;

    let hotkeys = [
        create_hotkey_field(hwnd, &file_config, 10, 104, 440, 560, 170)?,
        create_hotkey_field(hwnd, &file_config, 30, 138, 440, 560, 170)?,
        create_hotkey_field(hwnd, &file_config, 60, 172, 440, 560, 170)?,
        create_hotkey_field(hwnd, &file_config, 120, 206, 440, 560, 170)?,
        create_hotkey_field(hwnd, &file_config, 300, 240, 440, 560, 170)?,
    ];

    let save_button = create_button(hwnd, "Save", 580, 560, 80, 28)?;
    let cancel_button = create_button(hwnd, "Cancel", 670, 560, 80, 28)?;

    Ok(SettingsWindowState {
        window: hwnd.0 as isize,
        output_dir: output_dir.0 as isize,
        ffmpeg_path: ffmpeg_path.0 as isize,
        max_replay_seconds: max_replay_seconds.0 as isize,
        segment_seconds: segment_seconds.0 as isize,
        system_audio_enabled: system_audio_enabled.0 as isize,
        system_audio_backend: system_audio_backend.0 as isize,
        system_audio_device: system_audio_device.0 as isize,
        microphone_enabled: microphone_enabled.0 as isize,
        microphone_backend: microphone_backend.0 as isize,
        microphone_device: microphone_device.0 as isize,
        audio_sample_rate: audio_sample_rate.0 as isize,
        audio_channels: audio_channels.0 as isize,
        audio_bitrate: audio_bitrate.0 as isize,
        hotkeys,
        save_button: save_button.0 as isize,
        cancel_button: cancel_button.0 as isize,
    })
}

unsafe fn create_hotkey_field(
    hwnd: HWND,
    file_config: &FileConfig,
    duration_seconds: u32,
    y: i32,
    label_x: i32,
    edit_x: i32,
    edit_width: i32,
) -> Result<HotkeyFieldHandle> {
    create_label(
        hwnd,
        &format!("Save last {duration_seconds}s"),
        label_x,
        y + 4,
        110,
        20,
    )?;

    let combination = file_config
        .hotkeys
        .iter()
        .find(|entry| entry.duration_seconds == duration_seconds)
        .map(|entry| entry.combination.as_str())
        .or_else(|| default_hotkey_combination(duration_seconds))
        .unwrap_or("");

    let handle = create_edit(hwnd, combination, edit_x, y, edit_width, 24)?;
    Ok(HotkeyFieldHandle {
        duration_seconds,
        handle: handle.0 as isize,
    })
}

unsafe fn create_label(
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    create_child_window(
        parent,
        "STATIC",
        text,
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
    )
}

unsafe fn create_edit(
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    create_child_window(
        parent,
        "EDIT",
        text,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
        x,
        y,
        width,
        height,
    )
}

unsafe fn create_button(
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    create_child_window(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        x,
        y,
        width,
        height,
    )
}

unsafe fn create_checkbox(
    parent: HWND,
    text: &str,
    checked: bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    let hwnd = create_child_window(
        parent,
        "BUTTON",
        text,
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | 0x00000003),
        x,
        y,
        width,
        height,
    )?;
    set_checkbox_checked(hwnd, checked);
    Ok(hwnd)
}

unsafe fn create_child_window(
    parent: HWND,
    class_name: &str,
    text: &str,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    let hinstance = GetModuleHandleW(None)?;
    let class_name = wide_null(class_name);
    let text = wide_null(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(class_name.as_ptr()),
        PCWSTR(text.as_ptr()),
        style,
        x,
        y,
        width,
        height,
        Some(parent),
        None,
        Some(hinstance.into()),
        None,
    )?;
    Ok(hwnd)
}

unsafe fn save_settings_from_window(_settings_hwnd: HWND) -> Result<()> {
    let state = APP_STATE
        .get()
        .ok_or_else(|| anyhow!("application state not initialized"))?;
    let window = state
        .settings_window
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("settings window state missing"))?;

    let output_dir = read_control_text(hwnd_from_isize(window.output_dir))?;
    let ffmpeg_path = read_control_text(hwnd_from_isize(window.ffmpeg_path))?;
    let max_replay_seconds = parse_u32_field(
        "Replay buffer seconds",
        &read_control_text(hwnd_from_isize(window.max_replay_seconds))?,
    )?;
    let segment_seconds = parse_u32_field(
        "Segment length seconds",
        &read_control_text(hwnd_from_isize(window.segment_seconds))?,
    )?;
    let system_audio_enabled = read_checkbox_checked(hwnd_from_isize(window.system_audio_enabled));
    let microphone_enabled = read_checkbox_checked(hwnd_from_isize(window.microphone_enabled));
    let system_audio_backend = read_control_text(hwnd_from_isize(window.system_audio_backend))?;
    let system_audio_device = read_control_text(hwnd_from_isize(window.system_audio_device))?;
    let microphone_backend = read_control_text(hwnd_from_isize(window.microphone_backend))?;
    let microphone_device = read_control_text(hwnd_from_isize(window.microphone_device))?;
    let audio_sample_rate = parse_u32_field(
        "Audio sample rate",
        &read_control_text(hwnd_from_isize(window.audio_sample_rate))?,
    )?;
    let audio_channels = parse_u32_field(
        "Audio channels",
        &read_control_text(hwnd_from_isize(window.audio_channels))?,
    )?;
    let audio_bitrate = read_control_text(hwnd_from_isize(window.audio_bitrate))?;

    let mut file_config = load_or_create_file_config(&state.paths)?;
    file_config.output_dir = empty_to_none_path(&output_dir);
    file_config.ffmpeg_path = empty_to_none_path(&ffmpeg_path);
    file_config.max_replay_seconds = max_replay_seconds;
    file_config.segment_seconds = segment_seconds;
    file_config.system_audio_enabled = system_audio_enabled;
    file_config.system_audio_backend = system_audio_backend;
    file_config.system_audio_device = system_audio_device;
    file_config.microphone_enabled = microphone_enabled;
    file_config.microphone_backend = microphone_backend;
    file_config.microphone_device = microphone_device;
    file_config.audio_sample_rate = audio_sample_rate;
    file_config.audio_channels = audio_channels;
    file_config.audio_bitrate = audio_bitrate;
    file_config.hotkeys = window
        .hotkeys
        .iter()
        .map(|field| {
            let combination = read_control_text(hwnd_from_isize(field.handle))?;
            if combination.is_empty() {
                bail!(
                    "Hotkey for {}s replay cannot be blank",
                    field.duration_seconds
                );
            }
            Ok(HotkeyEntry {
                duration_seconds: field.duration_seconds,
                combination,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    apply_file_config(file_config)?;
    Ok(())
}

fn apply_file_config(file_config: FileConfig) -> Result<()> {
    let state = APP_STATE
        .get()
        .ok_or_else(|| anyhow!("application state not initialized"))?;
    let new_config = file_config.clone().into_app_config(&state.paths)?;
    ensure_runtime_dirs(&new_config, &state.paths)?;

    let main_hwnd = state
        .main_window
        .lock()
        .unwrap()
        .as_ref()
        .copied()
        .map(hwnd_from_isize)
        .ok_or_else(|| anyhow!("main window handle missing"))?;

    let mut runtime = state.runtime.lock().unwrap();
    let old_config = runtime.config.clone();
    let old_file_config = old_config.to_file_config(&state.paths);

    save_file_config(&state.paths, &file_config)?;

    unsafe { unregister_hotkeys(main_hwnd, &old_config) };
    if let Some(capture) = runtime.capture.take() {
        capture.stop();
    }

    if let Err(error) = unsafe { register_hotkeys(main_hwnd, &new_config) } {
        unsafe { unregister_hotkeys(main_hwnd, &new_config) };
        let _ = save_file_config(&state.paths, &old_file_config);
        let _ = unsafe { register_hotkeys(main_hwnd, &old_config) };
        runtime.capture = Some(CaptureSupervisor::start(Arc::new(old_config.clone())));
        return Err(error);
    }

    runtime.capture = Some(CaptureSupervisor::start(Arc::new(new_config.clone())));
    runtime.config = new_config;
    drop(runtime);

    unsafe {
        show_notification(
            main_hwnd,
            "Settings saved",
            "Capture restarted with the updated configuration.",
            false,
        );
    }

    Ok(())
}

fn empty_to_none_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn parse_u32_field(name: &str, value: &str) -> Result<u32> {
    value
        .trim()
        .parse::<u32>()
        .with_context(|| format!("{name} must be a whole number"))
}

unsafe fn read_control_text(hwnd: HWND) -> Result<String> {
    let length = GetWindowTextLengthW(hwnd);
    let mut buffer = vec![0u16; (length.max(0) as usize) + 1];
    let copied = GetWindowTextW(hwnd, &mut buffer) as usize;
    Ok(String::from_utf16_lossy(&buffer[..copied])
        .trim()
        .to_string())
}

unsafe fn set_checkbox_checked(hwnd: HWND, checked: bool) {
    let state = if checked { 1usize } else { 0usize };
    let _ = SendMessageW(hwnd, BM_SETCHECK, Some(WPARAM(state)), Some(LPARAM(0)));
}

unsafe fn read_checkbox_checked(hwnd: HWND) -> bool {
    SendMessageW(hwnd, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1
}

unsafe fn show_modal_message_box(hwnd: HWND, title: &str, body: &str, is_error: bool) {
    let title = wide_null(title);
    let body = wide_null(body);
    let style = if is_error {
        MB_OK | MB_ICONERROR
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    let _ = MessageBoxW(
        Some(hwnd),
        PCWSTR(body.as_ptr()),
        PCWSTR(title.as_ptr()),
        style,
    );
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

fn hwnd_from_isize(value: isize) -> HWND {
    HWND(value as *mut _)
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
