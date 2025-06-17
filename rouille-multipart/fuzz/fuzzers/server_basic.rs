use rouille_multipart::mock::ServerRequest;
use rouille_multipart::server::{Multipart, MultipartData};

mod logger;

use std::io::BufRead;

const BOUNDARY: &'static str = "--12--34--56";

#[export_name = "rust_fuzzer_test_input"]
pub extern "C" fn go(data: &[u8]) {
    logger::init();

    info!("Fuzzing started! Data len: {}", data.len());

    do_fuzz(data);

    info!("Finished fuzzing iteration");
}

fn do_fuzz(data: &[u8]) {
    if data.len() < BOUNDARY.len() {
        return;
    }

    let req = ServerRequest::new(data, BOUNDARY);

    info!("Request constructed!");

    let mut multipart = if let Ok(multi) = Multipart::from_request(req) {
        multi
    } else {
        panic!("This shouldn't have failed")
    };

    // A lot of requests will be malformed
    while let Ok(Some(entry)) = multipart.read_entry() {
        info!("read_entry() loop!");
        match entry.data {
            MultipartData::Text(_) => (),
            MultipartData::File(mut file) => loop {
                let consume = file.fill_buf().expect("This shouldn't fail").len();

                info!("Consume amt: {}", consume);

                if consume == 0 {
                    break;
                }
                file.consume(consume);
            },
        }
    }
}
