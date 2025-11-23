use once_cell::sync::OnceCell;
use std::path::PathBuf;

use tracing_appender::{
    non_blocking::{self, WorkerGuard},
    rolling::RollingFileAppender,
};
use tracing_subscriber::{
    filter::LevelFilter, fmt, prelude::*, reload, util::SubscriberInitExt, EnvFilter,
};

static FILE_FILTER_HANDLE: OnceCell<reload::Handle<EnvFilter, tracing_subscriber::Registry>> =
    OnceCell::new();
static _GUARD: OnceCell<WorkerGuard> = OnceCell::new(); // keep writer alive

fn log_dir() -> PathBuf {
    // ~/Library/Application Support/clip-downloader/logs (macOS)
    // ~/.config/clip-downloader/logs (Linux)
    // %APPDATA%\clip-downloader\logs (Windows)
    let base = dirs::config_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
    base.join("clip-downloader").join("logs")
}

/// Initialize global subscriber. Call once at app start.
pub fn init(file_enabled: bool) {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);

    // Daily rotation; current file is app.log and rotated copies per day.
    let file_appender: RollingFileAppender = tracing_appender::rolling::daily(dir, "app.log");
    let (nb_writer, guard): (non_blocking::NonBlocking, WorkerGuard) =
        tracing_appender::non_blocking(file_appender);

    // Keep background worker alive for the process lifetime.
    let _ = _GUARD.set(guard);

    // Always log to console (helpful for dev).
    let console = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_ansi(true);

    // File layer (no ANSI, include target + line for debugging).
    let file_layer = fmt::layer()
        .with_writer(nb_writer)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_line_number(true);

    // Make the file layer’s level reloadable so we can enable/disable at runtime.
    let initial = if file_enabled {
        EnvFilter::new("info")
    } else {
        EnvFilter::new("off")
    };
    let (reloadable_filter, handle) = reload::Layer::new(initial);
    let _ = FILE_FILTER_HANDLE.set(handle);

    // IMPORTANT: add the filtered file layer to the *registry* first (so its S = Registry),
    // then add the console layer. This avoids the trait bound error.
    tracing_subscriber::registry()
        .with(file_layer.with_filter(reloadable_filter)) // file on/off via settings
        .with(console.with_filter(LevelFilter::INFO)) // keep console at info+ to suppress noisy traces
        .init();

    prune_old_logs(); // optional small housekeeping
}

/// Enable/disable file logging after startup.
pub fn set_file_logging_enabled(enabled: bool) {
    if let Some(h) = FILE_FILTER_HANDLE.get() {
        let _ = h.modify(|f| {
            *f = if enabled {
                EnvFilter::new("info")
            } else {
                EnvFilter::new("off")
            };
        });
    }
}

/// Optional: keep the last ~10 rotated logs to avoid unbounded growth.
fn prune_old_logs() {
    use std::fs;

    let dir = log_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };

    let mut files: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| e.file_name().to_string_lossy().starts_with("app.log"))
        .collect();

    files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok()); // oldest first

    // keep newest 10, remove the rest
    if files.len() > 10 {
        let excess = files.len() - 10;
        for e in files.iter().take(excess) {
            let _ = fs::remove_file(e.path());
        }
    }
}
