extern crate multipart;
extern crate nickel;

use std::fs::File;
use std::io::{self, Read};
use nickel::{HttpRouter, MiddlewareResult, Nickel, Request, Response};

use multipart::nickel::MultipartBody;
use multipart::server::{Entries, SaveResult};
use multipart::mock::StdoutTee;

fn handle_multipart<'mw>(req: &mut Request, mut res: Response<'mw>) -> MiddlewareResult<'mw> {
    match req.multipart_body() {
        Some(mut multipart) => {
            match multipart.save().temp() {
                SaveResult::Full(entries) => process_entries(res, entries),

                SaveResult::Partial(entries, e) => {
                    println!("Partial errors ... {:?}", e);
                    return process_entries(res, entries.keep_partial());
                },

                SaveResult::Error(e) => {
                    println!("There are errors in multipart POSTing ... {:?}", e);
                    res.set(nickel::status::StatusCode::InternalServerError);
                    return res.send(format!("Server could not handle multipart POST! {:?}", e));
                },
            }
        }
        None => {
            res.set(nickel::status::StatusCode::BadRequest);
            return res.send("Request seems not was a multipart request")
        }
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries<'mw>(res: Response<'mw>, entries: Entries) -> MiddlewareResult<'mw> {
    if let Err(e) = entries.write_debug(StdoutTee::new(res.start()?)) {

    }

    res.send("Ok")
}

fn main() {
    let mut srv = Nickel::new();

    srv.post("/multipart_upload/", handle_multipart);

    // Start this example via:
    //
    // `cargo run --example nickel --features nickel`
    //
    // And - if you are in the root of this repository - do an example
    // upload via:
    //
    // `curl -F file=@LICENSE 'http://localhost:6868/multipart_upload/'`
    srv.listen("127.0.0.1:6868").expect("Failed to bind server");
}
