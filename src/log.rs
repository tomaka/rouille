// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Write;
use std::panic;
use std::time::Duration;
use std::time::Instant;

use chrono;

use Request;
use RawResponse;

/// Adds a log entry to the given writer at each request.
///
/// Each request writes a line to the given parameter. The line contains various info like the URL
/// of the request, the time taken, and the status code of the response.
///
/// # Example
///
/// ```
/// use std::io;
/// use rouille::{Request, Response, RawResponse};
///
/// fn handle(request: &Request) -> RawResponse {
///     rouille::log(request, io::stdout(), || {
///         Response::text("hello world")
///     })
/// }
/// ```
pub fn log<W, F, R>(rq: &Request, mut output: W, f: F) -> RawResponse
    where W: Write,
          F: FnOnce() -> R,
          R: Into<RawResponse>
{
    let start_instant = Instant::now();
    let rq_line = format!("{} UTC - {} {}", chrono::UTC::now().format("%Y-%m-%d %H:%M:%S%.6f"),
                                            rq.method(), rq.raw_url());

    // Calling the handler and catching potential panics.
    // Note that this we always resume unwinding afterwards, we can ignore the small panic-safety
    // mecanism of `catch_unwind`.
    let response = panic::catch_unwind(panic::AssertUnwindSafe(f));

    let elapsed_time = format_time(start_instant.elapsed());

    match response {
        Ok(response) => {
            let response: RawResponse = response.into();
            let _ = writeln!(output, "{} - {} - {}", rq_line, elapsed_time, response.status_code);
            response
        },
        Err(payload) => {
            // There is probably no point in printing the payload, as this is done by the panic
            // handler.
            let _ = writeln!(output, "{} - {} - PANIC!", rq_line, elapsed_time);
            panic::resume_unwind(payload);
        }
    }
}

fn format_time(duration: Duration) -> String {
    let secs_part = match duration.as_secs().checked_mul(1_000_000_000) {
        Some(v) => v,
        None => return format!("{}s", duration.as_secs() as f64),
    };

    let duration_in_ns = secs_part + duration.subsec_nanos() as u64;

    if duration_in_ns < 1_000 {
        format!("{}ns", duration_in_ns)
    } else if duration_in_ns < 1_000_000 {
        format!("{:.1}us", duration_in_ns as f64 / 1_000.0)
    } else if duration_in_ns < 1_000_000_000 {
        format!("{:.1}ms", duration_in_ns as f64 / 1_000_000.0)
    } else {
        format!("{:.1}s", duration_in_ns as f64 / 1_000_000_000.0)
    }
}
