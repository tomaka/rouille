extern crate log;

use self::log::{LogLevel, LogLevelFilter, Log, LogMetadata, LogRecord};

const MAX_LOG_LEVEL: LogLevel = LogLevel::Trace;

struct Logger;

impl Log for Logger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        metadata.level() <= MAX_LOG_LEVEL
    }

    fn log(&self, record: &LogRecord) {
        println!("{}: {}", record.level(), record.args());
    }
}

pub fn init() {
    let _ = log::set_logger(|max_lvl| {
        max_lvl.set(MAX_LOG_LEVEL.to_log_level_filter());
        Box::new(Logger)
    });
}
