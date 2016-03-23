extern crate multipart;
extern crate iron;

use std::fs::File;
use std::io::{Read};
use multipart::server::{Multipart, Entries, SaveResult};
use iron::prelude::*;
use iron::status;

fn main(){
    Iron::new(process_request).http("localhost:80").unwrap();
}

/// Processes a request and returns response or an occured error.
fn process_request(request: &mut Request) -> IronResult<Response> {
    // Getting a multipart reader wrapper
    match Multipart::from_request(request) {
        Ok(mut multipart) => {
            // Fetching all data and processing it.
            // save_all() reads the request fully, parsing all fields and saving all files
            // in a new temporary directory under the OS temporary directory.
            match multipart.save_all() {
                SaveResult::Full(entries) => process_entries(entries),
                SaveResult::Partial(entries, error) => {
                    try!(process_entries(entries));
                    Err(IronError::new(error, status::BadRequest))
                }
                SaveResult::Error(error) => Err(IronError::new(error, status::BadRequest)),
            }
        }
        Err(_) => Ok(Response::with((status::BadRequest, "The request is not multipart"))),
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries(entries: Entries) -> IronResult<Response> {
    for (name, field) in entries.fields {
        println!(r#"Field "{}": "{}""#, name, field);
    }

    for (name, savedfile) in entries.files {
        let filename = match savedfile.filename {
            Some(s) => s,
            None => "None".into(),
        };
        let mut file = match File::open(savedfile.path) {
            Ok(file) => file,
            Err(error) => return Err(IronError::new(error, status::BadRequest))
        };
        let mut contents = String::new();
        if let Err(error) = file.read_to_string(&mut contents) {
            return Err(IronError::new(error, status::BadRequest))
        }

        println!(r#"Field "{}" is file "{}":"#, name, filename);
        println!("{}", contents);
    }
    Ok(Response::with((status::Ok, "Multipart data is processed")))
}
