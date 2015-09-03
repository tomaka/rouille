use rustc_serialize::Decoder;
use rustc_serialize::Decodable;

use Request;
use RouteError;

use std::mem;
use std::num;
use std::str::ParseBoolError;

#[derive(Clone, Debug)]
pub enum PostError {
    WrongContentType,

    MissingField(String),

    WrongDataTypeBool(ParseBoolError),

    WrongDataTypeInt(num::ParseIntError),

    WrongDataTypeFloat(num::ParseFloatError),

    NotUtf8(String),
}

impl From<PostError> for RouteError {
    #[inline]
    fn from(err: PostError) -> RouteError {
        RouteError::WrongInput
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

pub fn get_post_input<T>(request: &Request) -> Result<T, PostError> where T: Decodable {
    // TODO: slow
    if request.header("Content-Type") != Some("application/x-www-form-urlencoded".to_owned()) {
        return Err(PostError::WrongContentType);
    }

    let mut decoder = PostDecoder::Start(request.data());
    T::decode(&mut decoder)
}

enum PostDecoder {
    Empty,

    Start(Vec<u8>),

    ExpectsStructMember(Vec<u8>),

    ExpectsData(Vec<u8>, String),
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
            &mut PostDecoder::ExpectsData(ref mut data, ref field_name) => {
                for element in data.split(|&c| c == b'&') {
                    let mut subsplit = element.splitn(2, |&c| c == b'=');

                    if subsplit.next().unwrap() == field_name.as_bytes() {
                        let value = subsplit.next().unwrap();

                        // FIXME: decode url encoding
                        return match String::from_utf8(value.to_owned()) {
                            Ok(value) => Ok(value),
                            Err(_) => Err(PostError::NotUtf8(field_name.clone())),
                        };
                    }
                }

                return Err(PostError::MissingField(field_name.clone()))
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

        f(&mut tmp)
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

    fn read_option<T, F>(&mut self, f: F) -> Result<T, PostError> where F: FnMut(&mut Self, bool) -> Result<T, PostError> {
        unimplemented!();
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
