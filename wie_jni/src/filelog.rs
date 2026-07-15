//! Writes every `log::info!`/`warn!`/`error!` call in this crate to a plain
//! text file, so you can inspect what happened without adb - just open the
//! file with any file manager / text editor.
//!
//! Also mirrors each line to logcat (tag `wie_jni`) via `android_log-sys`
//! directly, so `adb logcat -s wie_jni:V` still works when it's available;
//! the file is the primary channel, logcat is a bonus.

use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Write,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{Level, Log, Metadata, Record};

struct FileLogger {
    file: Mutex<Option<File>>,
}

static LOGGER: FileLogger = FileLogger { file: Mutex::new(None) };

/// `path` should be a full file path, e.g.
/// `/storage/emulated/0/wie/wie_jni.log`. Call once, as early as possible
/// (see `WieNative.kt`'s `init` block).
pub fn init(path: &str) {
    let open_result = std::path::Path::new(path)
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|()| OpenOptions::new().create(true).append(true).open(path));

    match open_result {
        Ok(f) => *LOGGER.file.lock().unwrap() = Some(f),
        Err(e) => {
            // Can't write the file (permission not granted yet, etc) - fall
            // back to logcat-only so we're not completely silent.
            android_log(Level::Error, &format!("wie_jni: failed to open log file {path}: {e} (check storage permission)"));
        }
    }

    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);

    log::info!("=== wie_jni log started, writing to {path} ===");
}

impl Log for FileLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let line = format!("[{millis}] {} {} - {}\n", record.level(), record.target(), record.args());

        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }

        android_log(record.level(), &format!("{} - {}", record.target(), record.args()));
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.flush();
        }
    }
}

fn android_log(level: Level, msg: &str) {
    let prio = match level {
        Level::Error => 6, // ANDROID_LOG_ERROR
        Level::Warn => 5,  // ANDROID_LOG_WARN
        Level::Info => 4,  // ANDROID_LOG_INFO
        Level::Debug => 3, // ANDROID_LOG_DEBUG
        Level::Trace => 2, // ANDROID_LOG_VERBOSE
    };
    let Ok(tag) = CString::new("wie_jni") else { return };
    let Ok(msg) = CString::new(msg) else { return };
    unsafe {
        android_log_sys::__android_log_write(prio, tag.as_ptr(), msg.as_ptr());
    }
}
