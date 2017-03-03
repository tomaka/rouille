extern crate nickel;
extern crate multipart;

use std::fs::File;
use std::io::Read;
use multipart::server::{Entries, Multipart, SaveResult};
use nickel::{HttpRouter, MiddlewareResult, Nickel, Request, Response};

fn handle_multipart<'mw>(req: &mut Request, mut res: Response<'mw>) -> MiddlewareResult<'mw> {
    match Multipart::from_request(req) {
        Ok(mut multipart) => {
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
        Err(_) => {
            res.set(nickel::status::StatusCode::BadRequest);
            return res.send("Request seems not was a multipart request")
        }
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries<'mw>(res: Response<'mw>, entries: Entries) -> MiddlewareResult<'mw> {
    for (name, field) in entries.fields {
        println!("Field {:?}: {:?}", name, field);
    }

    for (name, files) in entries.files {
        println!("Field {:?} has {} files:", name, files.len());

        for saved_file in files {
            match File::open(&saved_file.path) {
                Ok(mut file) => {
                    let mut contents = String::new();
                    if let Err(e) = file.read_to_string(&mut contents) {
                        println!("Could not read file {:?}. Error: {:?}", saved_file.filename, e);
                        return res.error(nickel::status::StatusCode::BadRequest, "The uploaded file was not readable")
                    }

                    println!("File {:?} ({:?}):", saved_file.filename, saved_file.content_type);
                    println!("{}", contents);
                    file
                }
                Err(e) => {
                    println!("Could open file {:?}. Error: {:?}", saved_file.filename, e);
                    return res.error(nickel::status::StatusCode::BadRequest, "The uploaded file was not readable")
                }
            };
        }
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
