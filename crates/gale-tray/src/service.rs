use std::ffi::OsStr;

use windows_service::service::{ServiceAccess, ServiceState as WinServiceState};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::Error as WinError;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::state::ServiceState;

pub const SERVICE_NAME: &str = "galed";

fn raw_os_error(error: &WinError) -> Option<i32> {
    match error {
        WinError::Winapi(io_error) => io_error.raw_os_error(),
        _ => None,
    }
}

fn is_access_denied(error: &WinError) -> bool {
    raw_os_error(error) == Some(ERROR_ACCESS_DENIED as i32)
}

fn is_service_missing(error: &WinError) -> bool {
    raw_os_error(error) == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32)
}

pub fn query() -> ServiceState {
    let manager =
        match ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::CONNECT) {
            Ok(manager) => manager,
            Err(_) => return ServiceState::Unknown,
        };

    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(service) => service,
        Err(error) if is_service_missing(&error) => return ServiceState::Missing,
        Err(_) => return ServiceState::Unknown,
    };

    match service.query_status() {
        Ok(status) => match status.current_state {
            WinServiceState::Running => ServiceState::Running,
            WinServiceState::Stopped => ServiceState::Stopped,
            WinServiceState::StartPending
            | WinServiceState::StopPending
            | WinServiceState::ContinuePending
            | WinServiceState::PausePending => ServiceState::Pending,
            WinServiceState::Paused => ServiceState::Unknown,
        },
        Err(_) => ServiceState::Unknown,
    }
}

pub fn start() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::CONNECT)
        .map_err(|error| error.to_string())?;

    match manager.open_service(SERVICE_NAME, ServiceAccess::START) {
        Ok(service) => service
            .start(&[] as &[&OsStr])
            .map_err(|error| error.to_string()),
        Err(error) if is_access_denied(&error) => relaunch_elevated("--start"),
        Err(error) => Err(error.to_string()),
    }
}

pub fn stop() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::CONNECT)
        .map_err(|error| error.to_string())?;

    match manager.open_service(SERVICE_NAME, ServiceAccess::STOP) {
        Ok(service) => service
            .stop()
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Err(error) if is_access_denied(&error) => relaunch_elevated("--stop"),
        Err(error) => Err(error.to_string()),
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn relaunch_elevated(arg: &str) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let current_exe = to_wide(&current_exe.to_string_lossy());
    let verb = to_wide("runas");
    let arg = to_wide(arg);

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            current_exe.as_ptr(),
            arg.as_ptr(),
            std::ptr::null(),
            SW_HIDE,
        )
    };

    if (result as isize) <= 32 {
        Err(format!("ShellExecuteW failed with code {result:?}"))
    } else {
        Ok(())
    }
}
