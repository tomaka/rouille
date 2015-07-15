use super::LogProvider;

use std::sync::Mutex;

use hyper::server::request::Request as HyperRequest;
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
    fn log_request(&self, request: &HyperRequest) {
        let mut out = self.out.lock().unwrap();

        // writing the method
        out.fg(color::GREEN).unwrap();
        match request.method {
            HyperMethod::Options => write!(out, "OPTIONS").unwrap(),
            HyperMethod::Get => write!(out, "GET").unwrap(),
            HyperMethod::Post => write!(out, "POST").unwrap(),
            HyperMethod::Put => write!(out, "PUT").unwrap(),
            HyperMethod::Delete => write!(out, "DELETE").unwrap(),
            HyperMethod::Head => write!(out, "HEAD").unwrap(),
            HyperMethod::Trace => write!(out, "TRACE").unwrap(),
            HyperMethod::Connect => write!(out, "CONNECT").unwrap(),
            HyperMethod::Patch => write!(out, "PATCH").unwrap(),
            HyperMethod::Extension(ref ext) => write!(out, "{} (custom)", ext).unwrap(),
        }
        write!(out, " ").unwrap();
        assert!(out.reset().unwrap());

        // writing the URI
        match request.uri {
            HyperRequestUri::AbsolutePath(ref p) => write!(out, "{}", p).unwrap(),
            HyperRequestUri::AbsoluteUri(ref uri) => write!(out, "{:?}", uri).unwrap(),     // TODO: handle better
            HyperRequestUri::Authority(ref auth) => write!(out, "{}", auth).unwrap(),
            HyperRequestUri::Star => write!(out, "*").unwrap(),
        }

        // finishing
        write!(out, "\n").unwrap();
    }
}
