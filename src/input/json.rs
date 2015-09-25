use rustc_serialize::Decodable;
use rustc_serialize::json;
use std::string::FromUtf8Error;
use Request;

/// Error that can happen when parsing the JSON input.
#[derive(Debug)]
pub enum JsonError {
    /// Wrong content type.
    WrongContentType,

    /// The request's body was not UTF8.
    NotUtf8(FromUtf8Error),

    /// Error while parsing.
    ParseError(json::DecoderError),
}

impl From<FromUtf8Error> for JsonError {
    fn from(err: FromUtf8Error) -> JsonError {
        JsonError::NotUtf8(err)
    }
}

impl From<json::DecoderError> for JsonError {
    fn from(err: json::DecoderError) -> JsonError {
        JsonError::ParseError(err)
    }
}

pub fn get_json_input<O>(request: &Request) -> Result<O, JsonError> where O: Decodable {
    // TODO: slow
    if let Some(header) = request.header("Content-Type"){
        if !header.starts_with("application/json") {
            return Err(JsonError::WrongContentType);
        }
    } else {
        return Err(JsonError::WrongContentType);
    }

    let content = try!(String::from_utf8(request.data()));
    let data = try!(json::decode(&content));
    Ok(data)
}
