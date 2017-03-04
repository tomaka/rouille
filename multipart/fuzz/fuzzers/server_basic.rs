#![no_main]
extern crate libfuzzer_sys;
extern crate multipart;

use multipart::server::{Multipart, MultipartData};
use multipart::mock::ServerRequest;

use std::io::BufRead;

const BOUNDARY: &'static str = "--12--34--56";

#[export_name="rust_fuzzer_test_input"]
pub extern fn go(data: &[u8]) {
    if data.len() < BOUNDARY.len() { return; }

    let req = ServerRequest::new(data, BOUNDARY);

    let mut multipart = if let Ok(multi) = Multipart::from_request(req) {
        multi
    } else {
        panic!("This shouldn't have failed")
    };

    // A lot of requests will be malformed
    while let Ok(Some(entry)) = multipart.read_entry() {
        match entry.data {
            MultipartData::Text(_) => (),
            MultipartData::File(mut file) => loop {
                let consume = file.fill_buf().expect("This shouldn't fail").len();

                if consume == 0 { break; }
                file.consume(consume);
            }
        }
    }
}
