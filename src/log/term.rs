use super::LogProvider;

use std::io::Write;
use std::sync::Mutex;

use hyper::method::Method as HyperMethod;
use hyper::uri::RequestUri as HyperRequestUri;

use term::StdoutTerminal;
use term::{self, color};

/// Provides logs for the terminal.
pub struct TermLog {
    out: Mutex<Box<StdoutTerminal>>,
}

impl TermLog {
    /// Initializes the logs system.
    pub fn new() -> TermLog {
        TermLog {
            out: Mutex::new(term::stdout().unwrap()),
        }
    }
}

impl LogProvider for TermLog {
    fn log_request(&self, method: &HyperMethod, uri: &HyperRequestUri, time_nanoseconds: u64) {
        let mut out = self.out.lock().unwrap();

        // writing the method
        out.fg(color::GREEN).unwrap();
        match method {
            &HyperMethod::Options => write!(out, "OPTIONS").unwrap(),
            &HyperMethod::Get => write!(out, "GET").unwrap(),
            &HyperMethod::Post => write!(out, "POST").unwrap(),
            &HyperMethod::Put => write!(out, "PUT").unwrap(),
            &HyperMethod::Delete => write!(out, "DELETE").unwrap(),
            &HyperMethod::Head => write!(out, "HEAD").unwrap(),
            &HyperMethod::Trace => write!(out, "TRACE").unwrap(),
            &HyperMethod::Connect => write!(out, "CONNECT").unwrap(),
            &HyperMethod::Patch => write!(out, "PATCH").unwrap(),
            &HyperMethod::Extension(ref ext) => write!(out, "{} (custom)", ext).unwrap(),
        }
        write!(out, " ").unwrap();
        assert!(out.reset().unwrap());

        // writing the URI
        match uri {
            &HyperRequestUri::AbsolutePath(ref p) => write!(out, "{}", p).unwrap(),
            &HyperRequestUri::AbsoluteUri(ref uri) => write!(out, "{:?}", uri).unwrap(),     // TODO: handle better
            &HyperRequestUri::Authority(ref auth) => write!(out, "{}", auth).unwrap(),
            &HyperRequestUri::Star => write!(out, "*").unwrap(),
        }
        write!(out, " - ").unwrap();

        // writing the time
        out.fg(color::BRIGHT_BLACK).unwrap();
        format_time(out.by_ref(), time_nanoseconds);
        assert!(out.reset().unwrap());

        // finishing
        write!(out, "\n").unwrap();
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
