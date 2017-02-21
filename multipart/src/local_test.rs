// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
use mock::{ClientRequest, HttpBuffer};

use server::{MultipartField, MultipartData, ReadEntry};

use rand::{self, Rng};

use std::collections::HashMap;
use std::convert::AsRef;
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;
use std::iter;

#[derive(Debug)]
struct TestFields {
    texts: HashMap<String, String>,
    files: HashMap<String, FileEntry>,
    nested: HashMap<String, Vec<FileEntry>>,
}

impl TestFields {
    fn check_field<M: ReadEntry>(&mut self, field: &mut MultipartField<M>) {
        match field.data {
            MultipartData::Text(ref text) => {
                let test_text = self.texts.remove(&field.name);

                assert!(
                    test_text.is_some(),
                    "Got text field that wasn't in original dataset: {:?} : {:?} ",
                    field.name, text.text
                );

                let test_text = test_text.unwrap();

                assert!(
                    text.text == test_text,
                    "Unexpected data for field {:?}: Expected {:?}, got {:?}",
                    field.name, test_text, text.text
                );
            },
            MultipartData::File(ref mut file) => {
                let test_file = self.files.remove(&field.name).unwrap();

                let mut bytes = Vec::with_capacity(test_file.data.0.len());
                file.read_to_end(&mut bytes).unwrap();

                assert!(bytes == test_file.data.0, "Unexpected data for file {:?}: Expected {:?}, Got {:?}",
                        field.name, String::from_utf8_lossy(&test_file.data.0),
                        String::from_utf8_lossy(&bytes)
                );
            },
            _ => (),
        }
    }

    fn assert_is_empty(&self) {
        assert!(self.texts.is_empty(), "Text fields were not exhausted! Text fields: {:?}", self.texts);
        assert!(self.files.is_empty(), "File fields were not exhausted! File fields: {:?}", self.files);
    }
}

#[derive(Debug)]
struct FileEntry {
    filename: Option<String>,
    data: PrintHex,
}

impl FileEntry {
    fn gen() -> Self {
        let filename = match gen_bool() {
            true => Some(gen_string()),
            false => None,
        };

        FileEntry {
            filename: filename,
            data: PrintHex(gen_bytes())
        }
    }

    fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(|s| &**s)
    }
}

struct PrintHex(Vec<u8>);

impl fmt::Debug for PrintHex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "["));

        let mut written = false;

        for byte in &self.0 {
            try!(write!(f, "{:X}", byte));

            if written {
                try!(write!(f, ", "));
            }

            written = true;
        }

        write!(f, "]")
    }
}

macro_rules! do_test (
    ($client_test:ident, $server_test:ident) => (
        let _ = ::env_logger::init();

        info!("Client Test: {:?} Server Test: {:?}", stringify!($client_test),
              stringify!($server_test));

        let mut test_fields = gen_test_fields();

        trace!("Fields for test: {:?}", test_fields);

        let buf = $client_test(&test_fields);

        trace!(
            "\n==Test Buffer Begin==\n{}\n==Test Buffer End==",
            String::from_utf8_lossy(&buf.buf)
        );

        $server_test(buf, &mut test_fields);

        test_fields.assert_is_empty();
    );
);

#[test]
fn reg_client_reg_server() {
    do_test!(test_client, test_server);
}

#[test]
fn reg_client_entry_server() {
    do_test!(test_client, test_server_entry_api);
}

#[test]
fn lazy_client_reg_server() {
    do_test!(test_client_lazy, test_server);
}

#[test]
fn lazy_client_entry_server() {
    do_test!(test_client_lazy, test_server_entry_api);
}

fn gen_test_fields() -> TestFields {
    const MIN_FIELDS: usize = 1;
    const MAX_FIELDS: usize = 3;

    let mut rng = rand::weak_rng();

    let texts_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);
    let files_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);
    let nested_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);

    TestFields {
        texts: (0..texts_count).map(|_| (gen_string(), gen_string())).collect(),
        files: (0..files_count).map(|_| (gen_string(), FileEntry::gen())).collect(),
        nested: (0..nested_count).map(|_| {
            let files_count = rng.gen_range(MIN_FIELDS, MAX_FIELDS);

            (
                gen_string(),
                (0..files_count).map(|_| FileEntry::gen()).collect()
            )
        }).collect()
    }
}

fn gen_bool() -> bool {
    rand::thread_rng().gen()
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

    let mut files = test_fields.files.iter();
    let mut nested_files = test_fields.nested.iter();

    let mut multipart = Multipart::from_request(request).unwrap();
   
    // Intersperse file fields amongst text fields
    for (name, text) in &test_fields.texts {
        if let Some((file_name, file)) = files.next() {
            multipart.write_stream(file_name, &mut &*file.data.0, file.filename(), None)
                .unwrap();
        }

        if let Some((file_name, files)) = nested_files.next() {
            let (data, boundary) = gen_nested_multipart(files);
            let mime = format!("multipart/mixed; boundary={}", boundary).parse().unwrap();
            multipart.write_stream(file_name, &mut &*data, None, Some(mime)).unwrap();
        }

        multipart.write_text(name, text).unwrap();    
    }

    // Write remaining files
    for (file_name, file) in files {
       multipart.write_stream(file_name, &mut &*file.data.0, None, None).unwrap();
    }

    for (file_name, files) in nested_files {
        let (data, boundary) = gen_nested_multipart(files);
        let mime = format!("multipart/mixed; boundary={}", boundary).parse().unwrap();
        multipart.write_stream(file_name, &mut &*data, None, Some(mime)).unwrap();
    }

    multipart.send().unwrap()
}

fn test_client_lazy(test_fields: &TestFields) -> HttpBuffer {
    use client::lazy::Multipart;

    let mut multipart = Multipart::new();

    let mut test_files = test_fields.files.iter();
    let mut nested_files = test_fields.nested.iter();

    for (name, text) in &test_fields.texts {
        for (file_name, file) in &mut test_files {
            multipart.add_stream(&**file_name, Cursor::new(&file.data.0), file.filename(), None);
        }

        if let Some((file_name, files)) = nested_files.next() {
            let (data, boundary) = gen_nested_multipart(files);
            let mime = format!("multipart/mixed; boundary={}", boundary).parse().unwrap();
            multipart.add_stream(&**file_name, Cursor::new(data), None as Option<&'static str>,
                                 Some(mime));
        }

        multipart.add_text(&**name, &**text);
    }

    for (file_name, file) in test_files {
        multipart.add_stream(&**file_name, Cursor::new(&file.data.0), None as Option<&str>, None);
    }

    for (file_name, files) in nested_files {
        let (data, boundary) = gen_nested_multipart(files);
        let mime = format!("multipart/mixed; boundary={}", boundary).parse().unwrap();
        multipart.add_stream(&**file_name, Cursor::new(data), None as Option<&'static str>,
                             Some(mime));
    }

    let mut prepared = multipart.prepare_threshold(None).unwrap();

    let mut buf = Vec::new();

    let boundary = prepared.boundary().to_owned();
    let content_len = prepared.content_len();

    prepared.read_to_end(&mut buf).unwrap();

    HttpBuffer::with_buf(buf, boundary, content_len)
}

fn test_server(buf: HttpBuffer, fields: &mut TestFields) {
    use server::Multipart;

    let server_buf = buf.for_server();

    if let Some(content_len) = server_buf.content_len {
        assert!(content_len == server_buf.data.len() as u64, "Supplied content_len different from actual");
    }

    let mut multipart = Multipart::from_request(server_buf)
        .unwrap_or_else(|_| panic!("Buffer should be multipart!"));

    while let Some(mut field) = multipart.read_entry().unwrap() {
        fields.check_field(&mut field);
    }
}

fn test_server_entry_api(buf: HttpBuffer, fields: &mut TestFields) {
    use server::Multipart;

    let server_buf = buf.for_server();

    if let Some(content_len) = server_buf.content_len {
        assert!(content_len == server_buf.data.len() as u64, "Supplied content_len different from actual");
    }

    let multipart = Multipart::from_request(server_buf)
        .unwrap_or_else(|_| panic!("Buffer should be multipart!"));

    let mut entry = multipart.into_entry().expect_alt("Expected entry, got none", "Error reading entry");
    fields.check_field(&mut entry);

    while let Some(entry_) = entry.next_entry().unwrap_opt() {
        entry = entry_;
        fields.check_field(&mut entry);
    }
}

fn gen_nested_multipart(files: &[FileEntry]) -> (Vec<u8>, String) {
    let mut out = Vec::new();
    let boundary = gen_string();

    write!(out, "Content-Type: multipart/mixed; boundary={boundary}\r\n\r\n \
    --{boundary}\r\n", boundary=boundary);

    let mut written = false;

    for file in files {
        if written {
            write!(out, "\r\n--{}\r\n", boundary);
        }

        write!(out, "Content-Type: application/octet-stream");

        if let Some(ref filename) = file.filename {
            write!(out, "; filename={}", filename);
        }

        write!(out, "\r\n\r\n");

        out.write_all(&file.data.0);

        written = true;
    }

    write!(out, "\r\n--{}--\r\n", boundary);

    (out, boundary)
}

