// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Write;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use Request;

/// RAII guard that ensures that a log entry corresponding to a request will be written.
///
/// # Example
///
/// ```no_run
/// rouille::start_server("localhost:80", move |request| {
///     let _entry = rouille::LogEntry::start(std::io::stdout(), request);
///
///     // process the request here
///
/// # panic!()
///     // <-- the log entry is automatically written at the end of the handler
/// });
/// ```
///
pub struct LogEntry<W> where W: Write {
    line: String,
    output: W,
    start_time: Instant,
}

impl<'a, W> LogEntry<W> where W: Write {
    /// Starts a `LogEntry`.
    pub fn start(output: W, rq: &Request) -> LogEntry<W> {
        LogEntry {
            line: format!("{} {}", rq.method(), rq.raw_url()),
            output: output,
            start_time: Instant::now(),
        }
    }
}

impl<W> Drop for LogEntry<W> where W: Write {
    fn drop(&mut self) {
        write!(self.output, "{} - ", self.line).unwrap();

        if thread::panicking() {
            write!(self.output, " - PANIC!").unwrap();

        } else {
            format_time(self.output.by_ref(), self.start_time.elapsed());
        }

        writeln!(self.output, "").unwrap();
    }
}

fn format_time<W>(mut out: W, duration: Duration) where W: Write {
    let secs_part = match duration.as_secs().checked_mul(1_000_000_000) {
        Some(v) => v,
        None => {
            write!(out, "{}s", duration.as_secs() as f64).unwrap();
            return;
        }
    };

    let duration_in_ns = secs_part + duration.subsec_nanos() as u64;

    if duration_in_ns < 1_000 {
        write!(out, "{}ns", duration_in_ns).unwrap()
    } else if duration_in_ns < 1_000_000 {
        write!(out, "{:.1}us", duration_in_ns as f64 / 1_000.0).unwrap()
    } else if duration_in_ns < 1_000_000_000 {
        write!(out, "{:.1}ms", duration_in_ns as f64 / 1_000_000.0).unwrap()
    } else {
        write!(out, "{:.1}s", duration_in_ns as f64 / 1_000_000_000.0).unwrap()
    }
}
