extern crate log;

use self::log::{LogLevelFilter, Log, LogMetadata, LogRecord};

const MAX_LOG_LEVEL: LogLevelFilter = LogLevelFilter::Off;

struct Logger;

impl Log for Logger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        metadata.level() <= MAX_LOG_LEVEL
    }

    fn log(&self, record: &LogRecord) {
        println!("{}: {}", record.level(), record.args());
    }
}

static LOGGER: Logger = Logger;

pub fn init() {
    let _ = unsafe {
        log::set_logger_raw(|max_lvl| {
            max_lvl.set(MAX_LOG_LEVEL);
            &LOGGER
        })
    };
}
