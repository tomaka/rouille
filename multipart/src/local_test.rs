// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
use mock::{ClientRequest, HttpBuffer};

use server::{MultipartField, MultipartData, ReadEntry};

use mime::Mime;

use rand::{self, Rng};

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;
use std::iter::{self, FromIterator};

const MIN_FIELDS: usize = 1;
const MAX_FIELDS: usize = 3;

const MIN_LEN: usize = 2;
const MAX_LEN: usize = 5;
const MAX_DASHES: usize = 2;

fn collect_rand<C: FromIterator<T>, T, F: FnMut() -> T>(mut gen: F) -> C {
    (0 .. rand::thread_rng().gen_range(MIN_FIELDS, MAX_FIELDS))
        .map(|_| gen()).collect()
}

macro_rules! expect_fmt (
    ($val:expr, $($args:tt)*) => (
        match $val {
            Some(val) => val,
            None => panic!($($args)*),
        }
    );
);

#[derive(Debug)]
struct TestFields {
    texts: HashMap<String, String>,
    files: HashMap<String, HashSet<FileEntry>>,
}

impl TestFields {
    fn gen() -> Self {
        TestFields {
            texts: collect_rand(|| (gen_string(), gen_string())),
            files: collect_rand(|| (gen_string(), FileEntry::gen_many())),
        }
    }

    fn check_field<M: ReadEntry>(&mut self, field: &mut MultipartField<M>) {
        match field.data {
            MultipartData::Text(ref text) => {
                let test_text = expect_fmt!(self.texts.remove(&field.name),
                    "Got text field that wasn't in original dataset: {:?} : {:?} ",
                    field.name, text.text
                );

                assert!(
                    text.text == test_text,
                    "Unexpected data for field {:?}: Expected {:?}, got {:?}",
                    field.name, test_text, text.text
                );
            },
            MultipartData::File(ref mut file) => {
                let mut bytes = Vec::with_capacity(MAX_LEN);
                file.read_to_end(&mut bytes).unwrap();

                let curr_file = FileEntry {
                    content_type: file.content_type.clone(),
                    filename: file.filename.take(),
                    data: PrintHex(bytes),
                };

                let files_empty = {
                    let mut files = expect_fmt!(self.files.get_mut(&field.name),
                    "Got file field that wasn't in original dataset: {:?} : {:?}",
                    field.name, curr_file);

                    assert!(files.remove(&curr_file), "Unexpected data for file field {:?}: {:?}",
                        field.name, curr_file);

                    files.is_empty()
                };

                if files_empty {
                    let _ = self.files.remove(&field.name);
                }
            },
        }
    }

    fn assert_is_empty(&self) {
        assert!(self.texts.is_empty(), "Text fields were not exhausted! Text fields: {:?}", self.texts);
        assert!(self.files.is_empty(), "File fields were not exhausted! File fields: {:?}", self.files);
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct FileEntry {
    content_type: Mime,
    filename: Option<String>,
    data: PrintHex,
}

impl FileEntry {
    fn gen_many() -> HashSet<Self> {
        collect_rand(Self::gen)
    }

    fn gen() -> Self {
        let filename = match gen_bool() {
            true => Some(gen_string()),
            false => None,
        };

        FileEntry {
            content_type: rand_mime(),
            filename: filename,
            data: PrintHex(gen_bytes())
        }
    }

    fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(|s| &**s)
    }
}

#[derive(Hash, PartialEq, Eq)]
struct PrintHex(Vec<u8>);

impl fmt::Debug for PrintHex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;

        let mut written = false;

        for byte in &self.0 {
            write!(f, "{:X}", byte)?;

            if written {
                write!(f, ", ")?;
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

        let mut test_fields = TestFields::gen();

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

mod extended {
    use super::{test_client, test_server, test_server_entry_api, test_client_lazy, TestFields};

    use std::time::Instant;

    const TIME_LIMIT_SECS: u64 = 300;

    #[test]
    #[ignore]
    fn reg_client_reg_server() {
        let started = Instant::now();

        while started.elapsed().as_secs() < TIME_LIMIT_SECS {
            do_test!(test_client, test_server);
        }
    }

    #[test]
    #[ignore]
    fn reg_client_entry_server() {
        let started = Instant::now();

        while started.elapsed().as_secs() < TIME_LIMIT_SECS {
            do_test!(test_client, test_server_entry_api);
        }
    }

    #[test]
    #[ignore]
    fn lazy_client_reg_server() {
        let started = Instant::now();

        while started.elapsed().as_secs() < TIME_LIMIT_SECS {
            do_test!(test_client_lazy, test_server);
        }
    }

    #[test]
    #[ignore]
    fn lazy_client_entry_server() {
        let started = Instant::now();

        while started.elapsed().as_secs() < TIME_LIMIT_SECS {
            do_test!(test_client_lazy, test_server_entry_api);
        }
    }
}


fn gen_bool() -> bool {
    rand::thread_rng().gen()
}

fn gen_string() -> String {
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
        if let Some((file_name, files)) = test_files.next() {
            for file in files {
                multipart.write_stream(file_name, &mut &*file.data.0, file.filename(),
                                       Some(file.content_type.clone())).unwrap();
            }
        }

        multipart.write_text(name, text).unwrap();    
    }

    // Write remaining files
    for (file_name, files) in test_files {
        for file in files {
            multipart.write_stream(file_name, &mut &*file.data.0, file.filename(),
                                   Some(file.content_type.clone())).unwrap();
        }
    }

    multipart.send().unwrap()
}

fn test_client_lazy(test_fields: &TestFields) -> HttpBuffer {
    use client::lazy::Multipart;

    let mut multipart = Multipart::new();

    let mut test_files = test_fields.files.iter();

    for (name, text) in &test_fields.texts {
        if let Some((file_name, files)) = test_files.next() {
            for file in files {
                multipart.add_stream(&**file_name, Cursor::new(&file.data.0), file.filename(),
                                     Some(file.content_type.clone()));
            }
        }

        multipart.add_text(&**name, &**text);
    }

    for (file_name, files) in test_files {
        for file in files {
            multipart.add_stream(&**file_name, Cursor::new(&file.data.0), file.filename(),
                                 Some(file.content_type.clone()));
        }
    }

    let mut prepared = multipart.prepare().unwrap();

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

    while let Some(mut field) = multipart.read_entry_mut().unwrap_opt() {
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

fn rand_mime() -> Mime {
    rand::thread_rng().choose(&[
        // TODO: fill this out, preferably with variants that may be hard to parse
        // i.e. containing hyphens, mainly
        mime!(Application/OctetStream),
        mime!(Text/Plain),
        mime!(Image/Png),
    ]).unwrap().clone()
}
