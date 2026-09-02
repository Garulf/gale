use galed::daemon::DaemonOptions;
use galed::shutdown::ShutdownSignal;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("restore") {
        let path = journal_path_from_args(&args);
        let restored = galed::claims_journal::run_restore(&path);
        tracing::info!(restored, path = %path.display(), "restore complete");
        return;
    }

    if args.first().map(String::as_str) == Some("--service") {
        #[cfg(windows)]
        {
            if let Err(error) = galed::service::run() {
                eprintln!("service failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(windows))]
        {
            eprintln!("--service is only available on Windows");
            std::process::exit(2);
        }
    }

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        galed::daemon::run(DaemonOptions {
            shutdown: ShutdownSignal::from_os(),
            on_ready: None,
        })
        .await;
    });
}

fn journal_path_from_args(args: &[String]) -> std::path::PathBuf {
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        if arg == "--journal" {
            if let Some(value) = rest.next() {
                return std::path::PathBuf::from(value);
            }
        }
    }
    galed::claims_journal::default_path()
}
