use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::{define_windows_service, service_dispatcher};

use crate::daemon::{self, DaemonOptions};
use crate::shutdown;

pub const SERVICE_NAME: &str = "galed";
pub const DISPLAY_NAME: &str = "Gale fan control";

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> Result<(), windows_service::Error> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        tracing::error!(%error, "windows service failed");
    }
}

fn start_pending_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    }
}

fn stop_pending_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    }
}

fn running_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn stopped_status(exit_code: ServiceExitCode) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn run_service() -> Result<(), windows_service::Error> {
    let (trigger, signal) = shutdown::channel();
    let trigger = Arc::new(trigger);
    let status_slot: Arc<Mutex<Option<ServiceStatusHandle>>> = Arc::new(Mutex::new(None));

    let handler_status_slot = status_slot.clone();
    let handler_trigger = trigger.clone();
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Some(handle) = *handler_status_slot.lock().unwrap() {
                    if let Err(error) = handle.set_service_status(stop_pending_status()) {
                        tracing::error!(%error, "failed to report StopPending status");
                    }
                }
                handler_trigger.fire();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
    *status_slot.lock().unwrap() = Some(status_handle);

    status_handle.set_service_status(start_pending_status())?;

    let running_status_handle = status_handle;
    let on_ready: Box<dyn FnOnce(std::net::SocketAddr) + Send> = Box::new(move |_addr| {
        if let Err(error) = running_status_handle.set_service_status(running_status()) {
            tracing::error!(%error, "failed to report Running status");
        }
    });

    let runtime = tokio::runtime::Runtime::new().expect("build tokio runtime");
    let outcome = runtime.block_on(daemon::run(DaemonOptions {
        shutdown: signal,
        on_ready: Some(on_ready),
    }));

    let exit_code = match &outcome {
        Ok(()) => ServiceExitCode::Win32(0),
        Err(error) => {
            tracing::error!(%error, "daemon run failed");
            ServiceExitCode::Win32(1)
        }
    };

    status_handle.set_service_status(stopped_status(exit_code))?;

    Ok(())
}
