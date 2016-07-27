extern crate tiny_http;
extern crate multipart;

use std::fs::File;
use std::io::{Error, Read};
use multipart::server::{Multipart, Entries, SaveResult};
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
fn process_request<'a, 'b>(request: &'a mut Request) -> Result<Response<&'b [u8]>, Error> {
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
                    Err(error)
                }
                SaveResult::Error(error) => Err(error),
            }
        }
        Err(_) => Ok(build_response(400, "The request is not multipart")),
    }
}

/// Processes saved entries from multipart request.
/// Returns an OK response or an error.
fn process_entries<'a>(entries: Entries) -> Result<Response<&'a [u8]>, Error> {
    for (name, field) in entries.fields {
        println!(r#"Field "{}": "{}""#, name, field);
    }

    for (name, savedfile) in entries.files {
        let filename = match savedfile.filename {
            Some(s) => s,
            None => "None".into(),
        };
        let mut file = try!(File::open(savedfile.path));
        let mut contents = String::new();
        try!(file.read_to_string(&mut contents));

        println!(r#"Field "{}" is file "{}":"#, name, filename);
        println!("{}", contents);
    }
    Ok(build_response(200, "Multipart data is received!".into()))
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
