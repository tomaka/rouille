extern crate tiny_http;
extern crate multipart;

use multipart::server::{Multipart, Entries, SaveResult};
fn main() {
    // Starting a server on `localhost:80`
    let server = tiny_http::Server::http("localhost:80").unwrap();
    loop {
        // This blocks until the next request is received
        let mut request = server.recv().unwrap();

        // Getting a multipart reader wrapper
        let mut multipart = Multipart::from_request(&mut request).unwrap();
        // Fetching all data and processing it
        match multipart.save_all() {
            SaveResult::Full(entries) => process_entries(entries).unwrap(),
            SaveResult::Partial(entries, error) => {
                process_entries(entries).unwrap();
                panic!("{:?}", error)
            },
            SaveResult::Error(error) => panic!("{:?}", error)
        }
    }
}

use std::io::prelude::*;
use std::fs::File;
use std::io::Error;
fn process_entries(entries: Entries) -> Result<(), Error> {
    for (name, field) in entries.fields {
        println!(r#"Field "{}": "{}""#, name, field);
    }

    for (name, savedfile) in entries.files {
        let filename = match savedfile.filename {
            Some(s) => s,
            None => "None".into()
        };
        let mut file = try!(File::open(savedfile.path));
        let mut contents = String::new();
        try!(file.read_to_string(&mut contents));

        println!(r#"Field "{}" is file "{}":"#, name, filename);
        println!("{}", contents);
    }
    Ok(())
}
