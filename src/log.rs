use std::io::Write;
use std::thread;

use time;

use Request;

/// RAII guard that ensures that a log entry corresponding to a request will be written.
///
/// # Example
///
/// ```no_run
/// # let server: rouille::Server = unsafe { std::mem::uninitialized() };
/// for request in server {
///     let _entry = rouille::LogEntry::start(std::io::stdout(), &request);
///
///     // process the request here
///
/// }   // <-- the log entry is written at the end of this block
/// ```
///
pub struct LogEntry<W> where W: Write {
    line: String,
    output: W,
    start_time: u64,
}

impl<'a, W> LogEntry<W> where W: Write {
    /// Starts a `LogEntry`.
    pub fn start(output: W, rq: &Request) -> LogEntry<W> {
        LogEntry {
            line: format!("\x1b[1mGET\x1b[0m {}", rq.url()),       // TODO: 
            output: output,
            start_time: time::precise_time_ns(),
        }
    }
}

impl<W> Drop for LogEntry<W> where W: Write {
    fn drop(&mut self) {
        write!(self.output, "{} - ", self.line).unwrap();

        if thread::panicking() {
            write!(self.output, " - \x1b[31;1mPANIC!\x1b[0m").unwrap();

        } else {
            let elapsed = time::precise_time_ns() - self.start_time;
            write!(self.output, "\x1b[90m").unwrap();
            format_time(self.output.by_ref(), elapsed);
            write!(self.output, "\x1b[0m").unwrap();
        }

        writeln!(self.output, "").unwrap();
    }
}

fn format_time<W>(mut out: W, time: u64) where W: Write {
    if time < 1_000 {
        write!(out, "{}ns", time).unwrap()
    } else if time < 1_000_000 {
        write!(out, "{:.1}us", time as f64 / 1_000.0).unwrap()
    } else if time < 1_000_000_000 {
        write!(out, "{:.1}ms", time as f64 / 1_000_000.0).unwrap()
    } else {
        write!(out, "{:.1}s", time as f64 / 1_000_000_000.0).unwrap()
    }
}
