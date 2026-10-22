//! A headless client for the download engine.
//!
//! ```text
//! idm-cli <url> [destination] [--connections N] [--proxy URL]
//! ```
//!
//! The client does the same work as the desktop application, without a
//! window: the file is split into ranges, the ranges are fetched in
//! parallel, and the parts are joined when they are all there.

use std::path::PathBuf;
use std::sync::Arc;

use idm_core::downloader::{Download, DownloadInfo, Reporter, Status};
use idm_core::limits::Limits;

/// A reporter that writes the progress to the terminal.
struct Console {
    /// The share of the file that was there when the last line was written.
    shown: std::sync::Mutex<f64>,
}

impl Console {
    fn new() -> Self {
        Self {
            shown: std::sync::Mutex::new(0.0),
        }
    }
}

impl Reporter for Console {
    fn report(&self, info: &DownloadInfo) {
        let mut shown = self.shown.lock().expect("shown lock poisoned");
        let share = if info.total > 0 {
            info.downloaded as f64 / info.total as f64
        } else {
            0.0
        };
        // A line every percent is enough to see that it moves.
        if share - *shown < 0.01 && info.status != Status::Completed {
            return;
        }
        *shown = share;

        let name = info
            .dest
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&info.dest)
            .to_string();
        match info.error.as_deref() {
            Some(error) => eprintln!("{name}: {error}"),
            None => println!(
                "{name}: {:.0}% ({:.1} MB of {:.1} MB)",
                share * 100.0,
                info.downloaded as f64 / 1_000_000.0,
                info.total as f64 / 1_000_000.0
            ),
        }
    }
}

fn usage() {
    eprintln!("usage: idm-cli <url> [destination] [--connections N] [--proxy URL]");
}

fn main() {
    let mut url = String::new();
    let mut destination: Option<String> = None;
    let mut connections: u32 = 8;
    let mut proxy: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--connections" => {
                let Some(value) = args.next() else {
                    usage();
                    std::process::exit(2);
                };
                connections = value.parse().unwrap_or(8);
            }
            "--proxy" => proxy = args.next(),
            "--help" | "-h" => {
                usage();
                return;
            }
            _ if url.is_empty() => url = arg,
            _ if destination.is_none() => destination = Some(arg),
            _ => {
                usage();
                std::process::exit(2);
            }
        }
    }

    if url.is_empty() {
        usage();
        std::process::exit(2);
    }

    let destination = destination.unwrap_or_else(|| file_name_of(&url));
    let path = PathBuf::from(destination);

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("idm-cli: {error}");
            std::process::exit(1);
        }
    };

    let exit = runtime.block_on(async move {
        let limits = Limits::default();
        limits.set_proxy(proxy.as_deref());

        let download = match Download::new(1, url.clone(), path, connections, limits.proxy().as_deref(), None)
        {
            Ok(download) => download,
            Err(error) => {
                eprintln!("idm-cli: {error}");
                return 1;
            }
        };

        idm_core::downloader::run(Arc::new(Console::new()), limits, download.clone()).await;

        let info = download.info();
        if info.status == Status::Completed {
            println!("done: {}", info.dest);
            0
        } else {
            eprintln!(
                "idm-cli: the download did not finish: {}",
                info.error.unwrap_or_else(|| "cancelled".to_string())
            );
            1
        }
    });

    std::process::exit(exit);
}

/// The file name of a URL, for the default destination.
fn file_name_of(url: &str) -> String {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let name = without_query
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("download.bin");
    // A name that is not a name on this system is not a name.
    if name.contains(':') || name.contains('*') || name.contains('?') {
        return "download.bin".to_string();
    }
    name.to_string()
}
