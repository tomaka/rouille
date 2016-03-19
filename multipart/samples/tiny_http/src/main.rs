extern crate tiny_http;
extern crate multipart;

use multipart::server::{Multipart, Entries, SaveResult};
use tiny_http::{Response, StatusCode, Request};
fn main() {
    // Starting a server on `localhost:80`
    let server = tiny_http::Server::http("localhost:80").unwrap();
    loop {
        // This blocks until the next request is received
        let mut request = server.recv().unwrap();
        process_multipart(&mut request).unwrap();

        // Answering with an HTTP OK and a string
        let response_string = "Multipart data is received!".as_bytes();
        let response = Response::new(StatusCode(200),
                                     vec![],
                                     response_string,
                                     Some(response_string.len()),
                                     None);
        request.respond(response).unwrap();
    }
}

fn process_multipart(request: &mut Request) -> Result<(), Error> {
    // Getting a multipart reader wrapper
    let mut multipart = Multipart::from_request(request).unwrap();
    // Fetching all data and processing it
    match multipart.save_all() {
        SaveResult::Full(entries) => process_entries(entries),
        SaveResult::Partial(entries, error) => {
            try!(process_entries(entries));
            Err(error)
        }
        SaveResult::Error(error) => Err(error),
    }
}
use std::fs::File;
use std::io::{Error, Read};
fn process_entries(entries: Entries) -> Result<(), Error> {
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
    Ok(())
}
