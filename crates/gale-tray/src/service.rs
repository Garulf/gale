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
            | WinServiceState::PausePending
            | WinServiceState::Paused => ServiceState::Pending,
        },
        Err(_) => ServiceState::Unknown,
    }
}

fn start_service() -> Result<(), WinError> {
    let manager = ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::START)?;
    service.start(&[] as &[&OsStr])
}

fn stop_service() -> Result<(), WinError> {
    let manager = ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::STOP)?;
    service.stop().map(|_| ())
}

pub fn start_direct() -> Result<(), String> {
    start_service().map_err(|error| error.to_string())
}

pub fn stop_direct() -> Result<(), String> {
    stop_service().map_err(|error| error.to_string())
}

pub fn start() -> Result<(), String> {
    match start_service() {
        Ok(()) => Ok(()),
        Err(error) if is_access_denied(&error) => relaunch_elevated("--start"),
        Err(error) => Err(error.to_string()),
    }
}

pub fn stop() -> Result<(), String> {
    match stop_service() {
        Ok(()) => Ok(()),
        Err(error) if is_access_denied(&error) => relaunch_elevated("--stop"),
        Err(error) => Err(error.to_string()),
    }
}

fn relaunch_elevated(arg: &str) -> Result<(), String> {
    use gale_pawnio::wide;

    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let current_exe = wide(&current_exe.to_string_lossy());
    let verb = wide("runas");
    let arg = wide(arg);

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
