//! This modules helps us with logging.

use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

use chrono::Local;

use log::{Level, LevelFilter, Metadata, Record, SetLoggerError};

use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Orbit, Request, Response, Rocket};

/// This structure holds the file where log will be appended.
pub struct Log {
    /// The file in which the logs will be appended.
    file: Arc<Mutex<File>>,

    /// Modules to log.
    modules: Vec<String>,
}

impl Log {
    /// Creates a new logging with a file.
    pub fn init(file: File, modules: Vec<String>) -> Result<(), SetLoggerError> {
        log::set_boxed_logger(Box::new(Log {
            file: Arc::new(Mutex::new(file)),
            modules,
        }))
        .map(|()| log::set_max_level(LevelFilter::Trace))?;
        Ok(())
    }

    fn includes_module(&self, module_path: &str) -> bool {
        // If modules is empty, include all module paths
        if self.modules.is_empty() {
            return true;
        }

        // if a prefix of module_path is in `self.modules`, it must
        // be located at the first location before
        // where module_path would be.
        let search = self
            .modules
            .binary_search_by(|module| module.as_str().cmp(&module_path));

        match search {
            Ok(_) => {
                // Found exact module: return true
                true
            }
            Err(0) => {
                // if there's no item which would be located before module_path, no prefix is there
                false
            }
            Err(i) => is_submodule(&self.modules[i - 1], module_path),
        }
    }
}

impl log::Log for Log {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.includes_module(metadata.target())
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let args = record.args();
            let now = Local::now().format("%d/%m/%Y %H:%M:%S");

            match record.level() {
                Level::Error => {
                    eprintln!("\x1b[38;5;243m{}\x1b[0m \x1b[31m[ERR] {}\x1b[0m", now, args);
                    writeln!(self.file.lock().unwrap(), "{} [ERR] {}", now, args).unwrap();
                }
                Level::Warn => {
                    eprintln!("\x1b[38;5;243m{}\x1b[0m \x1b[33m[WRN] {}\x1b[0m", now, args);
                    writeln!(self.file.lock().unwrap(), "{} [WRN] {}", now, args).unwrap();
                }
                Level::Info => {
                    eprintln!("\x1b[38;5;243m{}\x1b[0m \x1b[35m[LOG] {}\x1b[0m", now, args);
                    writeln!(self.file.lock().unwrap(), "{} [LOG] {}", now, args).unwrap();
                }
                Level::Debug => {
                    eprintln!("\x1b[38;5;243m{}\x1b[0m \x1b[34m[DBG] {}\x1b[0m", now, args);
                    writeln!(self.file.lock().unwrap(), "{} [DBG] {}", now, args).unwrap();
                }
                Level::Trace => {
                    eprintln!("\x1b[38;5;243m{}\x1b[0m \x1b[36m[TRC] {}\x1b[0m", now, args);
                    writeln!(self.file.lock().unwrap(), "{} [TRC] {}", now, args).unwrap();
                }
            }
        }
    }

    fn flush(&self) {
        self.file.lock().unwrap().flush().unwrap();
    }
}

fn is_submodule(parent: &str, possible_child: &str) -> bool {
    // Treat as bytes, because we'll be doing slicing, and we only care about ':' chars
    let parent = parent.as_bytes();
    let possible_child = possible_child.as_bytes();

    // a longer module path cannot be a parent of a shorter module path
    if parent.len() > possible_child.len() {
        return false;
    }

    // If the path up to the parent isn't the same as the child,
    if parent != &possible_child[..parent.len()] {
        return false;
    }

    // Either the path is exactly the same, or the sub module should have a "::" after
    // the length of the parent path. This prevents things like 'a::bad' being considered
    // a submodule of 'a::b'
    parent.len() == possible_child.len()
        || possible_child.get(parent.len()..parent.len() + 2) == Some(b"::")
}

/// Fairing to log responses to HTTP requests.
pub struct LogFairing;

#[rocket::async_trait]
impl Fairing for LogFairing {
    fn info(&self) -> Info {
        Info {
            name: "Log Fairing",
            kind: Kind::Liftoff | Kind::Response,
        }
    }

    async fn on_liftoff(&self, rocket: &Rocket<Orbit>) {
        info!("Server listening on port {}", rocket.config().port);
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        let ip = match req.client_ip() {
            Some(ip) => format!("{}", ip),
            None => String::from("Unknown addr"),
        };

        info!(
            "{} - {} {} {}",
            ip,
            req.method(),
            req.uri(),
            res.status().code
        );
    }
}
