//! runa internal library crate
//!
//! # This is not a public API.
//!
//! This library exists so that the runa 'rn' binary, the testing and the benchmarking
//! can share the same codebase. It is **not** intended to be used by externally.
//!
//! No guarantess are made about this library's API, and it may change at any time without notice.
//! The public API is the `rn` binary, which is the only supported way to use runa.
//! Nothing here is a supported interface; anything may change or be removed in any release,
//! including patch releases.
//!
//! Every module below is `pub(crate)`. The only externally reachable items are `run()`, the entry
//! point for the `rn` binary, and `bench_api`, which `benches/` needs because it is a separate
//! crate. `bench_api` is `#[doc(hidden)]` and nothing in `rn` reaches it.
//!
//! If you added `runa_tui` to your depenencies expecting a library, it will not give you one.

// runa contains no unsafe code. Enforced, not merely intended.
#![forbid(unsafe_code)]

pub(crate) mod app;
pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod core;
pub(crate) mod runtime;
pub(crate) mod ui;
pub(crate) mod utils;

#[doc(hidden)]
pub mod bench_api;

use std::io;
use std::panic;

use crate::cli::{CliAction, handle_args};
use crate::config::Config;

fn set_panic_hook() {
    panic::set_hook(Box::new(handle_panic));
}

fn handle_panic(info: &panic::PanicHookInfo<'_>) {
    let _ = crossterm::terminal::disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = crossterm::execute!(
        stdout,
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );

    eprintln!("\n[runa] Error occurred: {}", info);

    #[cfg(debug_assertions)]
    {
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!("\nStack Backtrace:\n{}", bt);
    }
}

fn load_config_or_default() -> Config {
    Config::load().unwrap_or_else(|e| {
        eprintln!("[runa] Config error: {}", e);
        Config::default()
    })
}

fn exit_with_startup_error(error: io::Error) -> ! {
    eprintln!("[runa] Error: {}", error);
    std::process::exit(1);
}

/// Runs runa
///
/// The single entry point for the `rn` binary.
/// It is not a supported interface. See the crate-level documentation for more information.
pub fn run() -> io::Result<()> {
    set_panic_hook();

    let action = handle_args();

    if let CliAction::Exit = action {
        return Ok(());
    }

    let config = load_config_or_default();

    let cli_paths = match action {
        CliAction::RunApp => None,
        CliAction::RunAppAtPath(paths) => Some(paths),
        _ => unreachable!(),
    };

    let mut runa = match runtime::start(config, cli_paths) {
        Ok(runa) => runa,
        Err(e) => exit_with_startup_error(e),
    };

    runtime::run_terminal(&mut runa)
}
