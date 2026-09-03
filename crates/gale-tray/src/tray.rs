use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer, PostMessageW,
    PostQuitMessage, SetTimer, TranslateMessage, HWND_MESSAGE, MSG, SW_SHOWNORMAL, WM_APP,
    WS_OVERLAPPED,
};

use crate::service;
use crate::state::{TrayState, UI_URL};

include!(concat!(env!("OUT_DIR"), "/icon_rgba.rs"));

const WM_TRAY_STATE: u32 = WM_APP + 1;
const MENU_POLL_TIMER_ID: usize = 1;
const MENU_POLL_INTERVAL_MS: u32 = 250;
const STATE_POLL_INTERVAL: Duration = Duration::from_secs(5);

struct MenuItems {
    status: MenuItem,
    open_ui: MenuId,
    start: MenuItem,
    stop: MenuItem,
    quit: MenuId,
}

pub fn run() -> Result<(), String> {
    let status = MenuItem::new("Gale: checking...", false, None);
    let open_ui_item = MenuItem::new("Open UI", true, None);
    let start = MenuItem::new("Start service", true, None);
    let stop = MenuItem::new("Stop service", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &open_ui_item,
        &start,
        &stop,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .map_err(|error| error.to_string())?;

    let icon = Icon::from_rgba(ICON_RGBA.to_vec(), ICON_WIDTH, ICON_HEIGHT)
        .map_err(|error| error.to_string())?;

    let items = MenuItems {
        status,
        open_ui: open_ui_item.id().clone(),
        start,
        stop,
        quit: quit_item.id().clone(),
    };

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("Gale fan control")
        .build()
        .map_err(|error| error.to_string())?;

    let hwnd = create_message_window()?;
    spawn_state_poller(hwnd);

    unsafe {
        SetTimer(hwnd, MENU_POLL_TIMER_ID, MENU_POLL_INTERVAL_MS, None);
    }

    run_message_loop(hwnd, &items)
}

fn create_message_window() -> Result<HWND, String> {
    let class_name = to_wide("STATIC");
    let window_name = to_wide("gale-tray-message-window");

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };

    if hwnd.is_null() {
        Err("CreateWindowExW failed".to_string())
    } else {
        Ok(hwnd)
    }
}

fn spawn_state_poller(hwnd: HWND) {
    let hwnd_addr = hwnd as isize;
    std::thread::spawn(move || {
        let hwnd = hwnd_addr as HWND;
        loop {
            let state = crate::probe::current_state();
            let payload = Box::into_raw(Box::new(state));
            let posted = unsafe { PostMessageW(hwnd, WM_TRAY_STATE, 0, payload as isize) };
            if posted == 0 {
                unsafe {
                    drop(Box::from_raw(payload));
                }
            }
            std::thread::sleep(STATE_POLL_INTERVAL);
        }
    });
}

fn run_message_loop(hwnd: HWND, items: &MenuItems) -> Result<(), String> {
    let mut msg = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }

        if msg.message == WM_TRAY_STATE {
            let state = unsafe { *Box::from_raw(msg.lParam as *mut TrayState) };
            apply_state(items, state);
        } else {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        handle_menu_events(items);
    }

    unsafe {
        KillTimer(hwnd, MENU_POLL_TIMER_ID);
        DestroyWindow(hwnd);
    }

    Ok(())
}

fn handle_menu_events(items: &MenuItems) {
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == items.open_ui {
            open_ui();
        } else if event.id == *items.start.id() {
            let _ = service::start();
        } else if event.id == *items.stop.id() {
            let _ = service::stop();
        } else if event.id == items.quit {
            unsafe {
                PostQuitMessage(0);
            }
        }
    }
}

fn apply_state(items: &MenuItems, state: TrayState) {
    items.status.set_text(state.status_label());
    items.start.set_enabled(state.can_start());
    items.stop.set_enabled(state.can_stop());
}

fn open_ui() {
    let operation = to_wide("open");
    let url = to_wide(UI_URL);
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
