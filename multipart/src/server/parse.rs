// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

extern crate httparse;

use self::httparse::{EMPTY_HEADER, Status};

use mime::Mime;

use std::io::{self, BufRead};
use std::str;

macro_rules! try_io(
    ($try:expr) => (
        {
            use std::io::{Error, ErrorKind};
            match $try {
                Ok(val) => val,
                Err(e) => return Err(Error::new(ErrorKind::InvalidData, format!("{:?}", e))),
            }
        }
    )
);

macro_rules! assert_log_ret_none (
    ($expr, $else_:expr) => (
        if !$expr {
            $else_;
            return None;
        }
    )
);

const EMPTY_STR_HEADER: StrHeader<'static> = StrHeader {
    name: "",
    val: "",
};

#[derive(Copy, Clone, Debug)]
struct StrHeader<'a> {
    name: &'a str,
    val: &'a str,
}

pub struct MultipartHeaders {
    pub cont_disp: ContentDisp,
    pub cont_type: Option<ContentType>,
}

impl MultipartHeaders {
    pub fn parse<R: BufRead>(r: &mut R) -> io::Result<Option<MultipartHeaders>> {
        const HEADER_LEN: usize = 4;

        // These are only written once so they don't need to be `mut` or initialized.
        let consume;
        let header_len;

        let mut headers = [EMPTY_STR_HEADER; HEADER_LEN];

        {
            let mut raw_headers = [EMPTY_HEADER; HEADER_LEN];

            loop {
                let buf = try!(r.fill_buf());

                match try_io!(httparse::parse_headers(buf, &mut raw_headers)) {
                    Status::Complete((consume_, raw_headers)) =>  {
                        consume = consume_;
                        header_len = raw_headers.len();
                        break;
                    },
                    Status::Partial => (),
                }
            }

            for (raw, header) in raw_headers.iter().take(header_len).zip(&mut headers) {
                header.name = raw.name;
                header.val = try!(io_str_utf8(raw.value));
            }
        }

        let headers = &headers[..header_len];

        debug!("Parsed field headers: {:?}", headers);

        r.consume(consume);

        Ok(Self::read_from(headers))
    }

    fn read_from(headers: &[StrHeader]) -> Option<MultipartHeaders> {
        let cont_disp = try_opt!(
                ContentDisp::read_from(headers),
                debug!("Failed to read Content-Disposition")
            );

        let cont_type = ContentType::read_from(headers);

        Some(MultipartHeaders {
            cont_disp: cont_disp,
            cont_type: cont_type,
        })
    }
}

pub struct ContentDisp {
    pub field_name: String,
    pub filename: Option<String>,
}

impl ContentDisp {
    fn read_from(headers: &[StrHeader]) -> Option<ContentDisp> {
        if headers.is_empty() {
            return None;
        }

        const CONT_DISP: &'static str = "Content-Disposition";

        let header = try_opt!(
            find_header(headers, CONT_DISP),
            error!("Field headers did not contain Content-Disposition header (required)")
        );

        const NAME: &'static str = "name=\"";
        const FILENAME: &'static str = "filename=\"";

        let after_disp_type = {
            let (disp_type, after_disp_type) = try_opt!(
                split_once(header.val, ';'),
                error!("Expected additional data after Content-Disposition type, got {:?}",
                header.val)
            );


            if disp_type.trim() != "form-data" {
                error!("Unexpected Content-Disposition value: {:?}", disp_type);
                return None;
            };

            after_disp_type
        };

        let (field_name, after_field_name) = try_opt!(
            get_str_after(NAME, '"', after_disp_type),
            error!("Expected field name and maybe filename, got {:?}", after_disp_type)
        );

        let filename = get_str_after(FILENAME, '"', after_field_name)
            .map(|(filename, _)| filename.to_owned());

        Some(ContentDisp { field_name: field_name.to_owned(), filename: filename })
    }
}

pub struct ContentType {
    pub val: Mime,
    #[allow(dead_code)]
    pub boundary: Option<String>,
}

impl ContentType {
    fn read_from(headers: &[StrHeader]) -> Option<ContentType> {
        const CONTENT_TYPE: &'static str = "Content-Type";

        let header = try_opt!(
            find_header(headers, CONTENT_TYPE),
            debug!("Content-Type header not found for field.")
        );

        const BOUNDARY: &'static str = "boundary=\"";

        if let Some((cont_type, after_cont_type)) = split_once(header.val, ';') {
            debug!("Found Content-Type: {:?}", cont_type);

            let content_type = read_content_type(cont_type.trim());

            let boundary = get_str_after(BOUNDARY, '"', after_cont_type)
                .map(|tup| tup.0.to_string());

            debug!("Found sub-boundary: {:?}", boundary);

            Some(ContentType {
                val: content_type,
                boundary: boundary,
            })
        } else {
            debug!("Found Content-Type: {:?}", header.val);
            let content_type = read_content_type(header.val.trim());
            Some(ContentType { val: content_type, boundary: None })
        }
    }
}

fn read_content_type(cont_type: &str) -> Mime {
    cont_type.parse().ok().unwrap_or_else(::mime_guess::octet_stream)
}


fn split_once(s: &str, delim: char) -> Option<(&str, &str)> {
    s.find(delim).map(|idx| s.split_at(idx))
}

/// Get the string after `needle` in `haystack`, stopping before `end_val_delim`
fn get_str_after<'a>(needle: &str, end_val_delim: char, haystack: &'a str) -> Option<(&'a str, &'a str)> {
    let val_start_idx = try_opt!(haystack.find(needle)) + needle.len();
    let val_end_idx = try_opt!(haystack[val_start_idx..].find(end_val_delim)) + val_start_idx;
    Some((&haystack[val_start_idx..val_end_idx], &haystack[val_end_idx..]))
}

fn io_str_utf8(buf: &[u8]) -> io::Result<&str> {
    str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn find_header<'a, 'b>(headers: &'a [StrHeader<'b>], name: &str) -> Option<&'a StrHeader<'b>> {
    headers.iter().find(|header| header.name == name)
}