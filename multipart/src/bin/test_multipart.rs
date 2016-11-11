extern crate multipart;

extern crate log;

use log::{LogRecord, LogMetadata, LogLevelFilter};

use multipart::server::Multipart;

use std::fs::File;
use std::env;

const LOG_LEVEL: LogLevelFilter = LogLevelFilter::Off;

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        LOG_LEVEL.to_log_level()
            .map_or(false, |level| metadata.level() <= level)
    }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }
}

fn main() {
    log::set_logger(|max_log_level| {
        max_log_level.set(LOG_LEVEL);
        Box::new(SimpleLogger)
    });

    let mut args = env::args().skip(1);

    let boundary = args.next().expect("Boundary must be provided as the first argument");

    let file = args.next().expect("Filename must be provided as the second argument");

    let file = File::open(file).expect("Could not open file");

    let mut multipart = Multipart::with_body(file, boundary);

    while let Some(field) = multipart.read_entry().unwrap() {
        println!("Read field: {:?}", field.name);
    }

    println!("All entries read!");
}