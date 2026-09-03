#[allow(dead_code)]
mod args;

#[allow(dead_code)]
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
        std::process::exit(1);
    }
}
