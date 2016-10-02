// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use rustc_serialize::Decoder;
use rustc_serialize::Decodable;

use Request;

use std::io::Error as IoError;
use std::io::Read;
use std::mem;
use std::num;
use std::str::ParseBoolError;
use url::form_urlencoded;

/// Error that can happen when decoding POST data.
#[derive(Debug)]
pub enum PostError {
    /// The `Content-Type` header of the request indicates that it doesn't contain POST data.
    WrongContentType,

    /// Can't parse the body of the request because it was already extracted.
    BodyAlreadyExtracted,

    /// Could not read the body from the request. Also happens if the body is not valid UTF-8.
    IoError(IoError),

    /// A field is missing from the received data.
    MissingField(String),

    /// Failed to parse a `bool` field.
    WrongDataTypeBool(ParseBoolError),

    /// Failed to parse an integer field.
    WrongDataTypeInt(num::ParseIntError),

    /// Failed to parse a floating-point field.
    WrongDataTypeFloat(num::ParseFloatError),

    /// Failed to parse a string field.
    NotUtf8(String),
}

impl From<IoError> for PostError {
    #[inline]
    fn from(err: IoError) -> PostError {
        PostError::IoError(err)
    }
}

impl From<ParseBoolError> for PostError {
    #[inline]
    fn from(err: ParseBoolError) -> PostError {
        PostError::WrongDataTypeBool(err)
    }
}

impl From<num::ParseIntError> for PostError {
    #[inline]
    fn from(err: num::ParseIntError) -> PostError {
        PostError::WrongDataTypeInt(err)
    }
}

impl From<num::ParseFloatError> for PostError {
    #[inline]
    fn from(err: num::ParseFloatError) -> PostError {
        PostError::WrongDataTypeFloat(err)
    }
}

/// Attempts to decode the `POST` data received by the request into a struct.
///
/// The struct must implement the `Decodable` trait from `rustc_serialize`.
///
/// An error is returned if a field is missing, if the content type is not POST data, or if a field
/// cannot be parsed.
///
/// # Example
///
/// ```
/// extern crate rustc_serialize;
/// # #[macro_use] extern crate rouille;
/// # use rouille::{Request, Response};
/// # fn main() {}
///
/// fn route_handler(request: &Request) -> Response {
///     #[derive(RustcDecodable)]
///     struct FormData {
///         field1: u32,
///         field2: String,
///     }
///
///     let data: FormData = try_or_400!(rouille::input::get_post_input(&request));
///     Response::text(format!("field1's value is {}", data.field1))
/// }
/// ```
///
pub fn get_post_input<T>(request: &Request) -> Result<T, PostError> where T: Decodable {
    let data = try!(get_raw_post_input(request));
    let mut decoder = PostDecoder::Start(data);
    T::decode(&mut decoder)
}

/// Attempts to decode the `POST` data received by the request.
///
/// If successful, returns a list of fields and values.
///
/// Returns an error if the request's content-type is not related to POST data.
// TODO: handle multipart here as well
pub fn get_raw_post_input(request: &Request) -> Result<Vec<(String, String)>, PostError> {
    // TODO: slow
    if request.header("Content-Type") != Some("application/x-www-form-urlencoded".to_owned()) {
        return Err(PostError::WrongContentType);
    }

    let body = {
        // TODO: DDoSable server if body is too large?
        let mut out = Vec::new();       // TODO: with_capacity()?
        if let Some(mut b) = request.data() {
            try!(b.read_to_end(&mut out));
        } else {
            return Err(PostError::BodyAlreadyExtracted);
        }
        out
    };

    Ok(form_urlencoded::parse(&body))
}

enum PostDecoder {
    Empty,

    Start(Vec<(String, String)>),

    ExpectsStructMember(Vec<(String, String)>),

    ExpectsData(Vec<(String, String)>, String),
}

impl Decoder for PostDecoder {
    type Error = PostError;

    fn read_usize(&mut self) -> Result<usize, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_u64(&mut self) -> Result<u64, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_u32(&mut self) -> Result<u32, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_u16(&mut self) -> Result<u16, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_u8(&mut self) -> Result<u8, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_isize(&mut self) -> Result<isize, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_i64(&mut self) -> Result<i64, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_i32(&mut self) -> Result<i32, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_i16(&mut self) -> Result<i16, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_i8(&mut self) -> Result<i8, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_bool(&mut self) -> Result<bool, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_f64(&mut self) -> Result<f64, PostError> { Ok(try!(try!(self.read_str()).parse())) }
    fn read_f32(&mut self) -> Result<f32, PostError> { Ok(try!(try!(self.read_str()).parse())) }

    fn read_char(&mut self) -> Result<char, PostError> {
        unimplemented!();
    }

    fn read_str(&mut self) -> Result<String, PostError> {
        match self {
            &mut PostDecoder::ExpectsData(ref data, ref field_name) => {
                let val = data.iter().find(|&&(ref key, _)| key == field_name)
                              .map(|&(_, ref value)| value);

                if let Some(val) = val {
                    Ok(val.clone())
                } else {
                    Err(PostError::MissingField(field_name.clone()))
                }
            },

            _ => panic!()
        }
    }

    fn read_nil(&mut self) -> Result<(), PostError> {
        unimplemented!();
    }

    fn read_enum<T, F>(&mut self, name: &str, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_enum_variant<T, F>(&mut self, names: &[&str], f: F) -> Result<T, PostError> where F: FnMut(&mut Self, usize) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_enum_variant_arg<T, F>(&mut self, a_idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_enum_struct_variant<T, F>(&mut self, names: &[&str], f: F) -> Result<T, PostError> where F: FnMut(&mut Self, usize) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_enum_struct_variant_field<T, F>(&mut self, f_name: &str, f_idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_struct<T, F>(&mut self, s_name: &str, len: usize, mut f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        let mut tmp = match mem::replace(self, PostDecoder::Empty) {
            PostDecoder::Start(data) => PostDecoder::ExpectsStructMember(data),
            _ => panic!()
        };

        f(&mut tmp)
    }

    fn read_struct_field<T, F>(&mut self, f_name: &str, f_idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        let mut tmp = match mem::replace(self, PostDecoder::Empty) {
            PostDecoder::ExpectsStructMember(data) => PostDecoder::ExpectsData(data, f_name.to_owned()),
            _ => panic!()
        };

        let result = f(&mut tmp);

        match tmp {
            PostDecoder::ExpectsData(data, _) => mem::replace(self, PostDecoder::ExpectsStructMember(data)),
            _ => panic!()
        };

        result
    }

    fn read_tuple<T, F>(&mut self, len: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_tuple_arg<T, F>(&mut self, a_idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_tuple_struct<T, F>(&mut self, s_name: &str, len: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_tuple_struct_arg<T, F>(&mut self, a_idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_option<T, F>(&mut self, mut f: F) -> Result<T, PostError> where F: FnMut(&mut Self, bool) -> Result<T, PostError> {
        let found = match self {
            &mut PostDecoder::ExpectsData(ref data, ref field_name) => {
                data.iter().find(|&&(ref key, _)| key == field_name).is_some()
            },
            _ => panic!()
        };

        f(self, found)
    }

    fn read_seq<T, F>(&mut self, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self, usize) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_seq_elt<T, F>(&mut self, idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_map<T, F>(&mut self, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self, usize) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_map_elt_key<T, F>(&mut self, idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }

    fn read_map_elt_val<T, F>(&mut self, idx: usize, f: F) -> Result<T, PostError> where F: FnOnce(&mut Self) -> Result<T, PostError> {
        unimplemented!();
    }


    fn error(&mut self, err: &str) -> PostError {
        unimplemented!();
    }
}
