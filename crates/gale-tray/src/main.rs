#[allow(dead_code)]
mod args;

#[allow(dead_code)]
mod state;

#[cfg(not(windows))]
fn main() {
    eprintln!("gale-tray runs on Windows only");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    match args::parse(&raw_args) {
        Ok(_command) => std::process::exit(0),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }
}
