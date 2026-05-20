#![windows_subsystem = "windows"]

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::mem::{size_of, transmute, zeroed};
use std::os::raw::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

type Bool = i32;
type Dword = u32;
type Handle = *mut c_void;
type Hbrush = *mut c_void;
type Hcursor = *mut c_void;
type Hicon = *mut c_void;
type Hinstance = *mut c_void;
type Hmenu = *mut c_void;
type Hwnd = *mut c_void;
type Lparam = isize;
type Lpcwstr = *const u16;
type Lresult = isize;
type Uint = u32;
type Wparam = usize;

const APP_NAME: &str = "Warmer";
const MUTEX_NAME: &str = "Local\\Warmer";
const WM_COMMAND: Uint = 0x0111;
const WM_DESTROY: Uint = 0x0002;
const WM_TIMER: Uint = 0x0113;
const WM_APP: Uint = 0x8000;
const WM_TRAY_ICON: Uint = WM_APP + 1;
const WM_LBUTTONUP: Uint = 0x0202;
const WM_RBUTTONUP: Uint = 0x0205;
const NIM_ADD: Dword = 0x0000_0000;
const NIM_MODIFY: Dword = 0x0000_0001;
const NIM_DELETE: Dword = 0x0000_0002;
const NIF_MESSAGE: Uint = 0x0000_0001;
const NIF_ICON: Uint = 0x0000_0002;
const NIF_TIP: Uint = 0x0000_0004;
const MF_STRING: Uint = 0x0000_0000;
const MF_SEPARATOR: Uint = 0x0000_0800;
const MF_CHECKED: Uint = 0x0000_0008;
const MF_DISABLED: Uint = 0x0000_0002;
const MF_POPUP: Uint = 0x0000_0010;
const TPM_LEFTALIGN: Uint = 0x0000_0000;
const TPM_RIGHTBUTTON: Uint = 0x0000_0002;
const MB_OK: Uint = 0x0000_0000;
const MB_ICONERROR: Uint = 0x0000_0010;
const ERROR_ALREADY_EXISTS: Dword = 183;
const IDI_APPLICATION: usize = 32512;
const LOAD_LIBRARY_SEARCH_SYSTEM32: Dword = 0x0000_0800;
const TIMER_ID: usize = 1;
const REAPPLY_INTERVAL_MS: Uint = 15_000;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
const ID_TOGGLE: Uint = 1001;
const ID_REAPPLY: Uint = 1002;
const ID_EXIT: Uint = 1003;
const ID_TEMP_BASE: Uint = 2000;
const ID_STRENGTH_BASE: Uint = 3000;
const TEMPERATURES: [i32; 7] = [2700, 3000, 3500, 4000, 4500, 5000, 6500];
const STRENGTHS: [i32; 4] = [50, 75, 85, 100];

static APP_STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
static MAG_INITIALIZED: AtomicBool = AtomicBool::new(false);
static MAG_FUNCTIONS: OnceLock<MagFunctions> = OnceLock::new();
static TASKBAR_CREATED_MESSAGE: OnceLock<Uint> = OnceLock::new();

#[derive(Clone, Copy)]
struct Settings {
    enabled: bool,
    temperature: i32,
    strength: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            temperature: 3500,
            strength: 100,
        }
    }
}

struct AppState {
    settings: Settings,
    settings_path: PathBuf,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: Uint,
    w_param: Wparam,
    l_param: Lparam,
    time: Dword,
    pt: Point,
    l_private: Dword,
}

#[repr(C)]
struct WndClassW {
    style: Uint,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, Uint, Wparam, Lparam) -> Lresult>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: Hinstance,
    h_icon: Hicon,
    h_cursor: Hcursor,
    hbr_background: Hbrush,
    lpsz_menu_name: Lpcwstr,
    lpsz_class_name: Lpcwstr,
}

#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
struct NotifyIconDataW {
    cb_size: Dword,
    hwnd: Hwnd,
    uid: Uint,
    uflags: Uint,
    ucallback_message: Uint,
    h_icon: Hicon,
    sz_tip: [u16; 128],
    dw_state: Dword,
    dw_state_mask: Dword,
    sz_info: [u16; 256],
    u_version: Uint,
    sz_info_title: [u16; 64],
    dw_info_flags: Dword,
    guid_item: Guid,
    h_balloon_icon: Hicon,
}

#[repr(C)]
struct MagColorEffect {
    transform: [f32; 25],
}

struct MagFunctions {
    initialize: MagInitializeFn,
    set_fullscreen_color_effect: MagSetFullscreenColorEffectFn,
    uninitialize: MagUninitializeFn,
}

type MagInitializeFn = unsafe extern "system" fn() -> Bool;
type MagSetFullscreenColorEffectFn = unsafe extern "system" fn(*const MagColorEffect) -> Bool;
type MagUninitializeFn = unsafe extern "system" fn() -> Bool;

#[link(name = "kernel32")]
extern "system" {
    fn CloseHandle(handle: Handle) -> Bool;
    fn CreateMutexW(attributes: *mut c_void, initial_owner: Bool, name: Lpcwstr) -> Handle;
    fn GetLastError() -> Dword;
    fn GetModuleHandleW(module_name: Lpcwstr) -> Hinstance;
    fn GetProcAddress(module: Handle, proc_name: *const u8) -> *mut c_void;
    fn LoadLibraryExW(file_name: Lpcwstr, file: Handle, flags: Dword) -> Handle;
}

#[link(name = "user32")]
extern "system" {
    fn AppendMenuW(menu: Hmenu, flags: Uint, new_item_id: usize, new_item: Lpcwstr) -> Bool;
    fn CreatePopupMenu() -> Hmenu;
    fn CreateWindowExW(
        ex_style: Dword,
        class_name: Lpcwstr,
        window_name: Lpcwstr,
        style: Dword,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, msg: Uint, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn DestroyMenu(menu: Hmenu) -> Bool;
    fn DestroyWindow(hwnd: Hwnd) -> Bool;
    fn DispatchMessageW(msg: *const Msg) -> Lresult;
    fn GetCursorPos(point: *mut Point) -> Bool;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min_filter: Uint, max_filter: Uint) -> Bool;
    fn KillTimer(hwnd: Hwnd, event_id: usize) -> Bool;
    fn LoadIconW(instance: Hinstance, icon_name: Lpcwstr) -> Hicon;
    fn MessageBoxW(hwnd: Hwnd, text: Lpcwstr, caption: Lpcwstr, kind: Uint) -> i32;
    fn PostQuitMessage(exit_code: i32);
    fn RegisterClassW(window_class: *const WndClassW) -> u16;
    fn RegisterWindowMessageW(string: Lpcwstr) -> Uint;
    fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
    fn SetProcessDpiAwarenessContext(value: Handle) -> Bool;
    fn SetTimer(hwnd: Hwnd, event_id: usize, elapsed_ms: Uint, timer_func: *const c_void) -> usize;
    fn TrackPopupMenu(
        menu: Hmenu,
        flags: Uint,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: Hwnd,
        rect: *const c_void,
    ) -> Bool;
    fn TranslateMessage(msg: *const Msg) -> Bool;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(message: Dword, data: *mut NotifyIconDataW) -> Bool;
}

fn main() {
    let reset_requested = env::args().any(|arg| arg.eq_ignore_ascii_case("--reset"));

    let mutex_name = wide(MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(null_mut(), 1, mutex_name.as_ptr()) };
    if mutex.is_null() {
        return;
    }

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(mutex);
        }
        return;
    }

    if reset_requested {
        let _ = unsafe { set_neutral() };
        shutdown_magnification();
        unsafe {
            CloseHandle(mutex);
        }
        return;
    }

    let result = unsafe { run() };
    if let Err(message) = result {
        show_message(null_mut(), &message);
    }

    shutdown_magnification();
    unsafe {
        CloseHandle(mutex);
    }
}

unsafe fn run() -> Result<(), String> {
    let app_dir = app_data_dir()?;
    fs::create_dir_all(&app_dir)
        .map_err(|error| format!("Could not create settings directory: {error}"))?;
    let settings_path = app_dir.join("settings.ini");
    let settings = load_settings(&settings_path);
    APP_STATE
        .set(Mutex::new(AppState {
            settings,
            settings_path,
        }))
        .map_err(|_| "Could not initialize app state.".to_string())?;

    if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 as Handle) == 0 {
        log_error(&last_error("SetProcessDpiAwarenessContext failed"));
    }

    let taskbar_created = wide("TaskbarCreated");
    let message_id = RegisterWindowMessageW(taskbar_created.as_ptr());
    if message_id != 0 {
        let _ = TASKBAR_CREATED_MESSAGE.set(message_id);
    }

    let instance = GetModuleHandleW(null());
    let class_name = wide("WarmerHiddenWindow");
    let window_class = WndClassW {
        style: 0,
        lpfn_wnd_proc: Some(window_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: load_app_icon(instance),
        h_cursor: null_mut(),
        hbr_background: null_mut(),
        lpsz_menu_name: null(),
        lpsz_class_name: class_name.as_ptr(),
    };

    if RegisterClassW(&window_class) == 0 {
        return Err(last_error("RegisterClassW failed"));
    }

    let title = wide(APP_NAME);
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        title.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        null_mut(),
    );

    if hwnd.is_null() {
        return Err(last_error("CreateWindowExW failed"));
    }

    add_or_update_tray_icon(hwnd, NIM_ADD)?;
    if SetTimer(hwnd, TIMER_ID, REAPPLY_INTERVAL_MS, null()) == 0 {
        return Err(last_error("SetTimer failed"));
    }

    if let Err(message) = apply_current_settings() {
        log_error(&message);
    }

    let mut msg: Msg = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    msg: Uint,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if TASKBAR_CREATED_MESSAGE
        .get()
        .is_some_and(|taskbar_created| msg == *taskbar_created)
    {
        let _ = add_or_update_tray_icon(hwnd, NIM_ADD);
        return 0;
    }

    match msg {
        WM_TRAY_ICON => {
            let mouse_message = lparam as Uint;
            if mouse_message == WM_RBUTTONUP || mouse_message == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            handle_command(hwnd, (wparam & 0xffff) as Uint);
            0
        }
        WM_TIMER => {
            if current_settings().enabled {
                if let Err(message) = apply_current_settings() {
                    log_error(&message);
                }
            }
            0
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_menu(hwnd: Hwnd) {
    let settings = current_settings();
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    let title = format!(
        "{} - {}, {}K, {}%",
        APP_NAME,
        if settings.enabled { "On" } else { "Off" },
        settings.temperature,
        settings.strength
    );
    append_item(menu, MF_STRING | MF_DISABLED, 0, &title);
    append_separator(menu);

    append_item(
        menu,
        MF_STRING | if settings.enabled { MF_CHECKED } else { 0 },
        ID_TOGGLE,
        if settings.enabled {
            "Enabled"
        } else {
            "Disabled"
        },
    );

    let temperature_menu = CreatePopupMenu();
    for (index, temperature) in TEMPERATURES.iter().enumerate() {
        append_item(
            temperature_menu,
            MF_STRING
                | if *temperature == settings.temperature {
                    MF_CHECKED
                } else {
                    0
                },
            ID_TEMP_BASE + index as Uint,
            &temperature_label(*temperature),
        );
    }
    append_popup(menu, temperature_menu, "Temperature");

    let strength_menu = CreatePopupMenu();
    for (index, strength) in STRENGTHS.iter().enumerate() {
        append_item(
            strength_menu,
            MF_STRING
                | if *strength == settings.strength {
                    MF_CHECKED
                } else {
                    0
                },
            ID_STRENGTH_BASE + index as Uint,
            &strength_label(*strength),
        );
    }
    append_popup(menu, strength_menu, "Strength");

    append_separator(menu);
    append_item(menu, MF_STRING, ID_REAPPLY, "Reapply now");
    append_item(menu, MF_STRING, ID_EXIT, "Exit");

    let mut point = Point { x: 0, y: 0 };
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
}

unsafe fn handle_command(hwnd: Hwnd, id: Uint) {
    match id {
        ID_TOGGLE => toggle_enabled(hwnd),
        ID_REAPPLY => apply_or_report(hwnd),
        ID_EXIT => exit_application(hwnd),
        id if id >= ID_TEMP_BASE && id < ID_TEMP_BASE + TEMPERATURES.len() as Uint => {
            let temperature = TEMPERATURES[(id - ID_TEMP_BASE) as usize];
            update_settings(hwnd, |settings| {
                settings.temperature = temperature;
                settings.enabled = true;
            });
        }
        id if id >= ID_STRENGTH_BASE && id < ID_STRENGTH_BASE + STRENGTHS.len() as Uint => {
            let strength = STRENGTHS[(id - ID_STRENGTH_BASE) as usize];
            update_settings(hwnd, |settings| {
                settings.strength = strength;
                settings.enabled = true;
            });
        }
        _ => {}
    }
}

unsafe fn toggle_enabled(hwnd: Hwnd) {
    update_settings(hwnd, |settings| settings.enabled = !settings.enabled);
}

unsafe fn update_settings(hwnd: Hwnd, update: impl FnOnce(&mut Settings)) {
    if let Some(state) = APP_STATE.get() {
        if let Ok(mut state) = state.lock() {
            update(&mut state.settings);
            let settings = state.settings;
            if let Err(error) = save_settings(&state.settings_path, settings) {
                log_error(&format!("Could not save settings: {error}"));
            }
        }
    }

    apply_or_report(hwnd);
    let _ = add_or_update_tray_icon(hwnd, NIM_MODIFY);
}

unsafe fn apply_or_report(hwnd: Hwnd) {
    if let Err(message) = apply_current_settings() {
        log_error(&message);
        show_message(hwnd, &message);
    }
}

unsafe fn exit_application(hwnd: Hwnd) {
    KillTimer(hwnd, TIMER_ID);
    if let Err(message) = set_neutral() {
        log_error(&message);
    }
    shutdown_magnification();
    remove_tray_icon(hwnd);
    DestroyWindow(hwnd);
}

unsafe fn add_or_update_tray_icon(hwnd: Hwnd, message: Dword) -> Result<(), String> {
    let instance = GetModuleHandleW(null());
    let mut data = notify_icon_data(hwnd, load_app_icon(instance), &tray_tip());
    if Shell_NotifyIconW(message, &mut data) == 0 {
        return Err(last_error("Shell_NotifyIconW failed"));
    }
    Ok(())
}

unsafe fn remove_tray_icon(hwnd: Hwnd) {
    let mut data = notify_icon_data(hwnd, null_mut(), "");
    Shell_NotifyIconW(NIM_DELETE, &mut data);
}

fn notify_icon_data(hwnd: Hwnd, icon: Hicon, tip: &str) -> NotifyIconDataW {
    let mut data: NotifyIconDataW = unsafe { zeroed() };
    data.cb_size = size_of::<NotifyIconDataW>() as Dword;
    data.hwnd = hwnd;
    data.uid = 1;
    data.uflags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.ucallback_message = WM_TRAY_ICON;
    data.h_icon = icon;
    copy_wide_to_fixed(tip, &mut data.sz_tip);
    data
}

fn tray_tip() -> String {
    let settings = current_settings();
    format!(
        "{} - {}, {}K, {}%",
        APP_NAME,
        if settings.enabled { "On" } else { "Off" },
        settings.temperature,
        settings.strength
    )
}

unsafe fn load_app_icon(instance: Hinstance) -> Hicon {
    let icon = LoadIconW(instance, make_int_resource(1));
    if icon.is_null() {
        LoadIconW(null_mut(), make_int_resource(IDI_APPLICATION as u16))
    } else {
        icon
    }
}

unsafe fn append_item(menu: Hmenu, flags: Uint, id: Uint, text: &str) {
    let wide_text = wide(text);
    AppendMenuW(menu, flags, id as usize, wide_text.as_ptr());
}

unsafe fn append_popup(menu: Hmenu, popup: Hmenu, text: &str) {
    let wide_text = wide(text);
    AppendMenuW(menu, MF_POPUP, popup as usize, wide_text.as_ptr());
}

unsafe fn append_separator(menu: Hmenu) {
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
}

fn current_settings() -> Settings {
    APP_STATE
        .get()
        .and_then(|state| state.lock().ok().map(|state| state.settings))
        .unwrap_or_default()
}

fn app_data_dir() -> Result<PathBuf, String> {
    env::var_os("LOCALAPPDATA")
        .map(|path| PathBuf::from(path).join(APP_NAME))
        .ok_or_else(|| {
            "LOCALAPPDATA is not set; cannot determine the settings directory.".to_string()
        })
}

fn load_settings(path: &Path) -> Settings {
    let mut settings = Settings::default();
    let Ok(content) = fs::read_to_string(path) else {
        return settings;
    };

    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim().to_ascii_lowercase().as_str() {
            "enabled" => {
                if let Ok(enabled) = value.trim().parse::<bool>() {
                    settings.enabled = enabled;
                }
            }
            "temperature" => {
                if let Ok(temperature) = value.trim().parse::<i32>() {
                    settings.temperature = temperature.clamp(1000, 10000);
                }
            }
            "strength" => {
                if let Ok(strength) = value.trim().parse::<i32>() {
                    settings.strength = strength.clamp(0, 100);
                }
            }
            _ => {}
        }
    }

    settings
}

fn save_settings(path: &Path, settings: Settings) -> Result<(), String> {
    let content = format!(
        "Enabled={}\nTemperature={}\nStrength={}\n",
        settings.enabled, settings.temperature, settings.strength
    );
    fs::write(path, content).map_err(|error| error.to_string())
}

unsafe fn apply_current_settings() -> Result<(), String> {
    let settings = current_settings();
    if settings.enabled {
        let scale = color_scale(settings.temperature, settings.strength);
        set_color_scale(scale.0, scale.1, scale.2)
    } else {
        set_neutral()
    }
}

unsafe fn set_neutral() -> Result<(), String> {
    set_color_scale(1.0, 1.0, 1.0)
}

unsafe fn set_color_scale(red: f32, green: f32, blue: f32) -> Result<(), String> {
    ensure_magnification_initialized()?;
    let effect = MagColorEffect {
        transform: [
            red, 0.0, 0.0, 0.0, 0.0, 0.0, green, 0.0, 0.0, 0.0, 0.0, 0.0, blue, 0.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    };

    let functions = load_magnification_functions()?;
    if (functions.set_fullscreen_color_effect)(&effect) == 0 {
        return Err(last_error("MagSetFullscreenColorEffect failed"));
    }
    Ok(())
}

unsafe fn ensure_magnification_initialized() -> Result<(), String> {
    if MAG_INITIALIZED.load(Ordering::SeqCst) {
        return Ok(());
    }

    let functions = load_magnification_functions()?;
    if (functions.initialize)() == 0 {
        return Err(last_error("MagInitialize failed"));
    }

    MAG_INITIALIZED.store(true, Ordering::SeqCst);
    Ok(())
}

fn shutdown_magnification() {
    if MAG_INITIALIZED.swap(false, Ordering::SeqCst) {
        unsafe {
            if let Some(functions) = MAG_FUNCTIONS.get() {
                (functions.uninitialize)();
            }
        }
    }
}

unsafe fn load_magnification_functions() -> Result<&'static MagFunctions, String> {
    if let Some(functions) = MAG_FUNCTIONS.get() {
        return Ok(functions);
    }

    let dll_name = wide("Magnification.dll");
    let module = LoadLibraryExW(dll_name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if module.is_null() {
        return Err(last_error("LoadLibraryExW(Magnification.dll) failed"));
    }

    let functions = MagFunctions {
        initialize: load_mag_initialize(module)?,
        set_fullscreen_color_effect: load_mag_set_fullscreen_color_effect(module)?,
        uninitialize: load_mag_uninitialize(module)?,
    };

    let _ = MAG_FUNCTIONS.set(functions);
    Ok(MAG_FUNCTIONS
        .get()
        .expect("magnification functions were just set"))
}

unsafe fn load_mag_initialize(module: Handle) -> Result<MagInitializeFn, String> {
    Ok(transmute::<*mut c_void, MagInitializeFn>(load_proc(
        module,
        b"MagInitialize\0",
    )?))
}

unsafe fn load_mag_set_fullscreen_color_effect(
    module: Handle,
) -> Result<MagSetFullscreenColorEffectFn, String> {
    Ok(transmute::<*mut c_void, MagSetFullscreenColorEffectFn>(
        load_proc(module, b"MagSetFullscreenColorEffect\0")?,
    ))
}

unsafe fn load_mag_uninitialize(module: Handle) -> Result<MagUninitializeFn, String> {
    Ok(transmute::<*mut c_void, MagUninitializeFn>(load_proc(
        module,
        b"MagUninitialize\0",
    )?))
}

unsafe fn load_proc(module: Handle, name: &'static [u8]) -> Result<*mut c_void, String> {
    let proc = GetProcAddress(module, name.as_ptr());
    if proc.is_null() {
        let proc_name = String::from_utf8_lossy(&name[..name.len().saturating_sub(1)]);
        return Err(last_error(&format!("GetProcAddress({proc_name}) failed")));
    }
    Ok(proc)
}

fn color_scale(kelvin: i32, strength: i32) -> (f32, f32, f32) {
    let raw = raw_color_scale(kelvin);
    let d65 = raw_color_scale(6500);
    let target = (
        clamp01(raw.0 / d65.0),
        clamp01(raw.1 / d65.1),
        clamp01(raw.2 / d65.2),
    );
    let amount = (strength.clamp(0, 100) as f32) / 100.0;
    (
        blend(1.0, target.0, amount),
        blend(1.0, target.1, amount),
        blend(1.0, target.2, amount),
    )
}

fn raw_color_scale(kelvin: i32) -> (f32, f32, f32) {
    let temperature = (kelvin.clamp(1000, 40000) as f64) / 100.0;
    let (red, green, blue) = if temperature <= 66.0 {
        let red = 255.0;
        let green = (99.4708025861 * temperature.ln()) - 161.1195681661;
        let blue = if temperature <= 19.0 {
            0.0
        } else {
            (138.5177312231 * (temperature - 10.0).ln()) - 305.0447927307
        };
        (red, green, blue)
    } else {
        let red = 329.698727446 * (temperature - 60.0).powf(-0.1332047592);
        let green = 288.1221695283 * (temperature - 60.0).powf(-0.0755148492);
        (red, green, 255.0)
    };

    (
        (red.clamp(0.0, 255.0) / 255.0) as f32,
        (green.clamp(0.0, 255.0) / 255.0) as f32,
        (blue.clamp(0.0, 255.0) / 255.0) as f32,
    )
}

fn blend(neutral: f32, target: f32, amount: f32) -> f32 {
    clamp01(neutral + ((target - neutral) * amount))
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn temperature_label(temperature: i32) -> String {
    match temperature {
        3500 => "3500K - default".to_string(),
        6500 => "6500K - neutral".to_string(),
        _ => format!("{temperature}K"),
    }
}

fn strength_label(strength: i32) -> String {
    match strength {
        50 => "Gentle - 50%".to_string(),
        75 => "Comfort - 75%".to_string(),
        85 => "Strong - 85%".to_string(),
        100 => "Full target - 100%".to_string(),
        _ => format!("{strength}%"),
    }
}

fn log_error(message: &str) {
    let directory = app_data_dir().unwrap_or_else(|_| env::temp_dir().join(APP_NAME));
    let path = directory.join("Warmer.log");
    let _ = fs::create_dir_all(&directory);
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{message}")
        });
}

fn show_message(hwnd: Hwnd, message: &str) {
    let text = wide(message);
    let caption = wide(APP_NAME);
    unsafe {
        MessageBoxW(hwnd, text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn last_error(context: &str) -> String {
    format!("{context}: Windows error {}", unsafe { GetLastError() })
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

fn copy_wide_to_fixed(text: &str, destination: &mut [u16]) {
    let encoded = wide(text);
    let len = encoded.len().min(destination.len());
    destination[..len].copy_from_slice(&encoded[..len]);
    if len == destination.len() {
        destination[destination.len() - 1] = 0;
    }
}

fn make_int_resource(id: u16) -> Lpcwstr {
    id as usize as Lpcwstr
}
