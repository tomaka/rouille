// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
use mock::{ClientRequest, HttpBuffer};

use rand::{self, Rng};

use std::collections::HashMap;
use std::io::prelude::*;
use std::io::Cursor;
use std::iter;

#[derive(Debug)]
struct TestFields {
    texts: HashMap<String, String>,
    files: HashMap<String, Vec<u8>>,
}

//#[test]
fn local_test() {
    do_test(test_client, "Regular");
}

#[test]
fn local_test_lazy() {
    do_test(test_client_lazy, "Lazy");
}

fn do_test(client: fn(&TestFields) -> HttpBuffer, name: &str) {
    let _ = ::env_logger::init();

    let test_fields = gen_test_fields();

    let buf = client(&test_fields);

    info!("Testing {} client", name);

    trace!(
        "\n==Test Buffer Begin==\n{}\n==Test Buffer End==",
        String::from_utf8_lossy(&buf.buf)
    );

    test_server(buf, test_fields);
}

fn gen_test_fields() -> TestFields {
    const MIN_FIELDS: usize = 1;
    const MAX_FIELDS: usize = 3;

    let mut rng = rand::weak_rng();

    let texts_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);
    let files_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);

    TestFields {
        texts: (0..texts_count).map(|_| (gen_string(), gen_string())).collect(),
        files: (0..files_count).map(|_| (gen_string(), gen_bytes())).collect(),
    }
}

fn gen_string() -> String {
    const MIN_LEN: usize = 2;
    const MAX_LEN: usize = 5;
    const MAX_DASHES: usize = 2;

    let mut rng_1 = rand::thread_rng();
    let mut rng_2 = rand::thread_rng();

    let str_len_1 = rng_1.gen_range(MIN_LEN, MAX_LEN + 1);
    let str_len_2 = rng_2.gen_range(MIN_LEN, MAX_LEN + 1);
    let num_dashes = rng_1.gen_range(0, MAX_DASHES + 1);

    rng_1.gen_ascii_chars().take(str_len_1)
        .chain(iter::repeat('-').take(num_dashes))
        .chain(rng_2.gen_ascii_chars().take(str_len_2))
        .collect()
}

fn gen_bytes() -> Vec<u8> {
    gen_string().into_bytes()
}

fn test_client(test_fields: &TestFields) -> HttpBuffer {
    use client::Multipart;

    let request = ClientRequest::default();

    let mut test_files = test_fields.files.iter();

    let mut multipart = Multipart::from_request(request).unwrap();
   
    // Intersperse file fields amongst text fields
    for (name, text) in &test_fields.texts {
        for (file_name, file) in &mut test_files {
            multipart.write_stream(file_name, &mut &**file, Some(file_name), None).unwrap();
        }

        multipart.write_text(name, text).unwrap();    
    }

    // Write remaining files
    for (file_name, file) in test_files {
       multipart.write_stream(file_name, &mut &**file, None, None).unwrap();
    }

    multipart.send().unwrap()
}

fn test_client_lazy(test_fields: &TestFields) -> HttpBuffer {
    use client::lazy::Multipart;

    let mut multipart = Multipart::new();

    let mut test_files = test_fields.files.iter();

    for (name, text) in &test_fields.texts {
        for (file_name, file) in &mut test_files {
            multipart.add_stream(&**file_name, Cursor::new(file), Some(&**file_name), None);
        }

        multipart.add_text(&**name, &**text);
    }

    for (file_name, file) in test_files {
        multipart.add_stream(&**file_name, Cursor::new(file), None as Option<&str>, None);
    }

    let mut prepared = multipart.prepare().unwrap();

    let mut buf = Vec::new();

    let boundary = prepared.boundary().to_owned();
    let content_len = prepared.content_len();

    prepared.read_to_end(&mut buf).unwrap();

    HttpBuffer::with_buf(buf, boundary, content_len)
}

fn test_server(buf: HttpBuffer, mut fields: TestFields) {
    use server::{Multipart, MultipartData};

    let server_buf = buf.for_server();

    if let Some(content_len) = server_buf.content_len {
        assert!(content_len == server_buf.data.len() as u64, "Supplied content_len different from actual");
    }

    let mut multipart = Multipart::from_request(server_buf)
        .unwrap_or_else(|_| panic!("Buffer should be multipart!"));

    trace!("Fields for server test: {:?}", fields);

    while let Ok(Some(mut field)) = multipart.read_entry() {
        match field.data {
            MultipartData::Text(text) => {
                let test_text = fields.texts.remove(&field.name);

                assert!(
                    test_text.is_some(),
                    "Got text field that wasn't in original dataset: {:?} : {:?} ",
                    field.name, text
                );

                let test_text = test_text.unwrap();

                assert!(
                    text == test_text, 
                    "Unexpected data for field {:?}: Expected {:?}, got {:?}", 
                    field.name, test_text, text
                );

            },
            MultipartData::File(ref mut file) => {
                let test_bytes = fields.files.remove(&field.name).unwrap();

                let mut bytes = Vec::with_capacity(test_bytes.len());
                file.read_to_end(&mut bytes).unwrap();

                assert!(bytes == test_bytes, "Unexpected data for file {:?}: Expected {:?}, Got {:?}", 
                        field.name, String::from_utf8_lossy(&test_bytes), String::from_utf8_lossy(&bytes)
                );
            },
        }
    }

    assert!(fields.texts.is_empty(), "Text fields were not exhausted! Text fields: {:?}", fields.texts);
    assert!(fields.files.is_empty(), "File fields were not exhausted! File fields: {:?}", fields.files);
}



