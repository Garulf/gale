#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg_attr(not(windows), allow(dead_code))]
mod args;

#[cfg_attr(not(windows), allow(dead_code))]
mod state;

#[cfg(windows)]
mod probe;

#[cfg(windows)]
mod service;

#[cfg(windows)]
mod tray;

#[cfg(not(windows))]
fn main() {
    eprintln!("gale-tray runs on Windows only");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args::parse(&raw_args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let result = match command {
        args::Command::Tray => tray::run(),
        args::Command::Start => service::start_direct(),
        args::Command::Stop => service::stop_direct(),
    };

    if let Err(message) = result {
        eprintln!("{message}");
        show_error_message_box(&message);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_error_message_box(message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let to_wide = |value: &str| -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };

    let text = to_wide(message);
    let title = to_wide("Gale tray");

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
