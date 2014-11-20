use hyper::header::common::content_type::ContentType;
use hyper::server::request::Request;

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use super::{MultipartField, MultipartFile};

use std::io::{RefReader, BufferedReader, IoError, IoResult, EndOfFile, standard_error, OtherIoError};

use std::kinds::marker;

fn is_multipart_formdata(req: &Request) -> bool {
    use mime::{Multipart};

    req.headers.get::<ContentType>().map(|ct| {
        let ContentType(ref mime) = *ct;
        match *mime {
            Mime(TopLevel::Multipart, SubLevel::Ext(ref subtype), _) => subtype[] == "form-data",
            _ => false,   
        }
    }).unwrap_or(false)
}

fn get_boundary(ct: &ContentType) -> Option<String> {
    let ContentType(ref mime) = *ct;
    let Mime(_, _, ref params) = *mime;
    
    params.iter().find(|&&(ref name, _)| 
        if let Attr::Ext(ref name) = *name { 
            name[] == "boundary" 
        } else { false }
    ).and_then(|&(_, ref val)| 
        if let Value::Ext(ref val) = *val { 
            Some(val.clone()) 
        } else { None }
    )        
}

pub struct Multipart {
    source: BoundaryReader,
}

macro_rules! try_find(
    ($haystack:expr, $f:ident, $needle:expr, $err:expr, $line:expr) => (
        try!($haystack.$f($needle).ok_or(line_error($err, $line.clone())))
    )
)

impl Multipart {

    /// If the given `Request` is of `Content-Type: multipart/form-data`, return
    /// the wrapped request as `Ok(Multipart)`, otherwise `Err(Request)`.
    pub fn from_request(req: Request) -> Result<Multipart, Request> {
        if !is_multipart_formdata(&req) { return Err(req); }

        let boundary = if let Some(boundary) = req.headers.get::<ContentType>()
            .and_then(get_boundary) { boundary } else { return Err(req); };

        Ok(Multipart { source: BoundaryReader::from_request(req, boundary) })
    }

    pub fn read_entry<'a>(&'a mut self) -> IoResult<(String, MultipartField<'a>)> {
        let (disp_type, field_name, filename) = try!(self.read_content_disposition());

        if &*disp_type != "form-data" {
            return Err(IoError {
                    kind: OtherIoError,
                    desc: "Content-Disposition value was not \"form-data\"",
                    detail: Some(format!("Content-Disposition: {}", disp_type)),
                });
        }
      
        if let Some(content_type) = try!(self.read_content_type()) {
            let _ = try!(self.source.reader.read_line()); // Consume empty line
            Ok((field_name, MultipartField::File(
                MultipartFile::from_octet(filename, &mut self.source, content_type[])))
            )
        } else {
            // Empty line consumed by read_content_type()
            let text = try!(self.source.read_to_string());
            Ok((field_name, MultipartField::Text(text)))
        }                
    }
   
    /// Call `f` for each entry in the multipart request.
    /// This is a substitute for `Multipart` implementing `Iterator`,
    /// since `Iterator::next()` can't use bound lifetimes.
    /// See https://www.reddit.com/r/rust/comments/2lkk4i/concrete_lifetime_vs_bound_lifetime/
    pub fn foreach_entry<F: for<'a> FnMut(String, MultipartField<'a>)>(&mut self, mut f: F) {
        loop {
            match self.read_entry() {
                Ok((name, field)) => f(name, field),
                Err(err) => { 
                    if err.kind != EndOfFile {
                        error!("Error reading Multipart: {}", err);
                    }

                    break;
                },            
            }    
        }    
    } 
    
    fn read_content_disposition(&mut self) -> IoResult<(String, String, Option<String>)> {
        let line = try!(self.source.reader.read_line());       

        // Find the end of CONT_DISP in the line
        let disp_type = {
            const CONT_DISP: &'static str = "Content-Disposition:";

            let disp_idx = try_find!(line[], find_str, CONT_DISP, 
                "Content-Disposition subheader not found!", line) + CONT_DISP.len(); 

            let disp_type_end = try_find!(line[disp_idx..], find, ';', 
                "Error parsing Content-Disposition value!", line);

            line[disp_idx .. disp_type_end].trim().into_string()
        };
    
        let field_name = {
            const NAME: &'static str = "name=\"";

            let name_idx = try_find!(line[], find_str, NAME, 
                "Error parsing field name!", line) + NAME.len();

            let name_end = try_find!(line[name_idx ..], find, '"',
                "Error parsing field name!", line);

            line[name_idx .. name_end].into_string() // No trim here since it's in quotes.
        };

        let filename = {
            const FILENAME: &'static str = "filename=\"";

            let filename_idx = line[].find_str(FILENAME).map(|idx| idx + FILENAME.len());
            let filename_idxs = with(filename_idx, |&start| line[start ..].find('"'));
            
            filename_idxs.map(|(start, end)| line[start .. end].into_string())
        };
        
        Ok((disp_type, field_name, filename))
    }

    fn read_content_type(&mut self) -> IoResult<Option<String>> {
        let line = try!(self.source.reader.read_line());

        const CONTENT_TYPE: &'static str = "Content-Type:";
        
        let type_idx = (&*line).find_str(CONTENT_TYPE);

        // FIXME Will not properly parse for multiple files! 
        // Does not expect boundary=<boundary>
        Ok(type_idx.map(|start| line[start + CONTENT_TYPE.len()..].trim().into_string()))
    }
}

fn with<T, U>(left: Option<T>, right: |&T| -> Option<U>) -> Option<(T, U)> {
    let temp = left.as_ref().and_then(right);
    match (left, temp) {
        (Some(lval), Some(rval)) => Some((lval, rval)),
        _ => None,    
    }
} 

fn line_error(msg: &'static str, line: String) -> IoError {
    IoError { 
        kind: OtherIoError, 
        desc: msg,
        detail: Some(line),
    }
}

/* FIXME: Can't have an iterator return a borrowed reference
impl<'a> Iterator<(String, MultipartField<'a>)> for Multipart {
    fn next(&mut self) -> Option<(String, MultipartField<'a>)> {
        match self.read_entry() {
            Ok(ok) => Some(ok), 
            Err(err) => { 
                if err.kind != EndOfFile {
                    error!("Error reading Multipart: {}", err);
                }

                None
             },
        }
    }    
}
*/

/// A `Reader` that will yield bytes until it sees a given sequence.
pub struct BoundaryReader {
    reader: BufferedReader<Request>,
    boundary: Vec<u8>,
    last_search_idx: uint,
    boundary_read: bool,
}

impl BoundaryReader {
    fn from_request(request: Request, mut boundary: String) -> BoundaryReader {
        boundary.prepend("--");

        BoundaryReader {
            reader: BufferedReader::new(request),
            boundary: boundary.into_bytes(),
            last_search_idx: 0,
            boundary_read: false,    
        }
    }

    fn read_to_boundary(&mut self) -> IoResult<bool> {
         if !self.boundary_read {
            let lookahead = try!(self.reader.fill_buf());
        
            self.last_search_idx = lookahead[self.last_search_idx..]
                .position_elem(&b'-').unwrap_or(lookahead.len() - 1);

            Ok(lookahead[self.last_search_idx..].starts_with(self.boundary[]))
        } else if self.last_search_idx == 0 {
            Err(standard_error(EndOfFile))                
        } else { Ok(true) } 
    }

    fn consume_boundary(&mut self) {
        self.reader.consume(self.last_search_idx + self.boundary.len());
        self.last_search_idx = 0;
        self.boundary_read = false;    
    }

    fn set_boundary(&mut self, boundary: String) {
        self.boundary = boundary.into_bytes();    
    }
}

impl Reader for BoundaryReader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        use std::cmp;        

        self.boundary_read = try!(self.read_to_boundary());

        let trunc_len = cmp::min(buf.len(), self.last_search_idx);

        let trunc_buf = buf[mut ..trunc_len]; // Truncate the buffer so we don't read ahead

        let bytes_read = self.reader.read(trunc_buf).unwrap();
        self.last_search_idx -= bytes_read;

        Ok(bytes_read)        
    } 
}

trait Prepend<T> {
    fn prepend(&mut self, t: T);    
}

impl<S: Str> Prepend<S> for String {
    fn prepend(&mut self, s: S) {
        unsafe {
            self.as_mut_vec().prepend(s.as_slice().as_bytes());    
        }      
    }
}

impl<'a, T> Prepend<&'a [T]> for Vec<T> {
    fn prepend(&mut self, slice: &[T]) {
        use std::ptr::copy_memory;

        let old_len = self.len();

        self.reserve(slice.len());

        unsafe {
            self.set_len(old_len + slice.len());
            copy_memory(self[mut slice.len()..].as_mut_ptr(), self[..old_len].as_ptr(), old_len);
            copy_memory(self.as_mut_ptr(), slice.as_ptr(), slice.len());
        }
    }    
}

#[test]
fn test_prepend() {
    let mut vec = vec![3u64, 4, 5];
    vec.prepend(&[1u64, 2]);
    assert_eq!(vec[], [1u64, 2, 3, 4, 5][]);
}

#[test]
fn test_prepend_string() {
    let mut string = "World!".into_string();
    string.prepend("Hello, ");
    assert_eq!(&*string, "Hello, World!");
}

