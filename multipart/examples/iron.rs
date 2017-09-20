extern crate multipart;
extern crate iron;

extern crate env_logger;

use std::fs::File;
use std::io::Read;
use multipart::server::{Multipart, Entries, SaveResult, SavedFile};
use iron::prelude::*;
use iron::status;

fn main() {
    env_logger::init().unwrap();

    Iron::new(process_request).http("localhost:80").expect("Could not bind localhost:80");
}

/// Processes a request and returns response or an occured error.
fn process_request(request: &mut Request) -> IronResult<Response> {
    // Getting a multipart reader wrapper
    match Multipart::from_request(request) {
        Ok(mut multipart) => {
            // Fetching all data and processing it.
            // save().temp() reads the request fully, parsing all fields and saving all files
            // in a new temporary directory under the OS temporary directory.
            match multipart.save().temp() {
                SaveResult::Full(entries) => process_entries(entries),
                SaveResult::Partial(entries, reason) => {
                    process_entries(entries.keep_partial())?;
                    Ok(Response::with((
                        status::BadRequest,
                        format!("error reading request: {}", reason.unwrap_err())
                    )))
                }
                SaveResult::Error(error) => Ok(Response::with((
                    status::BadRequest,
                    format!("error reading request: {}", error)
                ))),
            }
        }
        Err(_) => {
            Ok(Response::with((status::BadRequest, "The request is not multipart")))
        }
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries(entries: Entries) -> IronResult<Response> {
    for (name, field) in entries.fields {
        println!("Field {:?}: {:?}", name, field);
    }

    for (name, files) in entries.files {
        println!("Field {:?} has {} files:", name, files.len());

        for file in files {
            print_file(&file)?;
        }
    }

    Ok(Response::with((status::Ok, "Multipart data is processed")))
}

fn print_file(saved_file: &SavedFile) -> IronResult<()> {
    let mut file = match File::open(&saved_file.path) {
        Ok(file) => file,
        Err(error) => {
            return Err(IronError::new(error,
                                      (status::InternalServerError,
                                       "Server couldn't open saved file")))
        }
    };

    let mut contents = String::new();
    if let Err(error) = file.read_to_string(&mut contents) {
        return Err(IronError::new(error, (status::BadRequest, "The file was not a text")));
    }

    println!("File {:?} ({:?}):", saved_file.filename, saved_file.content_type);
    println!("{}", contents);

    Ok(())
}
