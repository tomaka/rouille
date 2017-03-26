extern crate tiny_http;
extern crate multipart;

use std::fs::File;
use std::io::{self, Read};
use multipart::server::{Multipart, Entries, SaveResult, SavedFile};
use tiny_http::{Response, StatusCode, Request};
fn main() {
    // Starting a server on `localhost:80`
    let server = tiny_http::Server::http("localhost:80").expect("Could not bind localhost:80");
    loop {
        // This blocks until the next request is received
        let mut request = server.recv().unwrap();

        // Processes a request and returns response or an occured error
        let result = process_request(&mut request);
        let resp = match result {
            Ok(resp) => resp,
            Err(e) => {
                println!("An error has occured during request proccessing: {:?}", e);
                build_response(500, "The received data was not correctly proccessed on the server")
            }
        };

        // Answers with a response to a client
        request.respond(resp).unwrap();
    }
}

/// Processes a request and returns response or an occured error.
fn process_request<'a, 'b>(request: &'a mut Request) -> io::Result<Response<&'b [u8]>> {
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
                    // We don't set limits
                    Err(reason.unwrap_err())
                }
                SaveResult::Error(error) => Err(error),
            }
        }
        Err(_) => Ok(build_response(400, "The request is not multipart")),
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries<'a>(entries: Entries) -> io::Result<Response<&'a [u8]>> {
    for (name, field) in entries.fields {
        println!("Field {:?}: {:?}", name, field);
    }

    for (name, files) in entries.files {
        println!("Field {:?} has {} files:", name, files.len());

        for file in files {
            print_file(&file)?;
        }
    }

    Ok(build_response(200, "Multipart data is received!"))
}

fn print_file(saved_file: &SavedFile) -> io::Result<()> {
    let mut file = File::open(&saved_file.path)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    println!("File {:?} ({:?}):", saved_file.filename, saved_file.content_type);
    println!("{}", contents);

    Ok(())
}

/// A utility function to build responses using only two arguments
fn build_response(status_code: u16, response: &str) -> Response<&[u8]> {
    let bytes = response.as_bytes();
    Response::new(StatusCode(status_code),
                  vec![],
                  bytes,
                  Some(bytes.len()),
                  None)
}
