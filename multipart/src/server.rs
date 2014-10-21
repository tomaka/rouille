use hyper::header::common::content_type::ContentType;
use hyper::server::request::Request;

use mime::{Mime, AttrExt, ValueExt};

use super::MultipartField;

use std::io::{RefReader, BufferedReader, IoResult, EndOfFile, standard_error};

fn is_multipart_formdata(req: &Request) -> bool {
    use mime::{Multipart, Mime, SubExt};

    req.headers.get::<ContentType>().map(|ct| {
        let ContentType(ref mime) = *ct;
        match *mime {
            Mime(Multipart, SubExt(ref subtype), _) => subtype[] == "form-data",
            _ => false,   
        }
    }).unwrap_or(false)
}

fn get_boundary(ct: &ContentType) -> Option<String> {
    let ContentType(ref mime) = *ct;
    let Mime(_, _, ref params) = *mime;
    
    params.iter().find(|&&(ref name, _)| 
        if let AttrExt(ref name) = *name { 
            name[] == "boundary" 
        } else { false }
    ).and_then(|&(_, ref val)| 
        if let ValueExt(ref val) = *val { 
            Some(val.clone()) 
        } else { None }
    )        
}

pub struct Multipart {
    source: BoundaryReader,
}

impl Multipart {

    /// If the given `Request` is of `Content-Type: multipart/form-data`, return
    /// the wrapped request as `Ok(Multipart)`, otherwise `Err(Request)`.
    pub fn from_request(req: Request) -> Result<Multipart, Request> {
        if !is_multipart_formdata(&req) { return Err(req); }
        let boundary = try!(req.headers.get::<ContentType>().and_then(get_boundary).ok_or_else(|| req));

        Ok(Multipart { source: BoundaryReader::from_request(req, boundary.into_bytes()) })
    }
    
    fn read_content_disposition(&mut self) -> (   
}

impl<'a> Iterator<(String, MultipartField<'a>)> for Multipart {
    fn next(&'a mut self) -> Option<(String, MultipartField<'a>)> {
        unimplemented!();           
    }    
}


/// A `Reader` that will yield bytes until it sees a given sequence.
pub struct BoundaryReader {
    reader: BufferedReader<Request>,
    boundary: Vec<u8>,
    last_search_idx: uint,
    boundary_read: bool,
}

impl BoundaryReader {
    fn from_request(request: Request, boundary: String) -> BoundaryReader {
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
        self.reader.consume(self.boundary.len());
        self.last_search_idx = 0;
        self.boundary_read = false;    
    }

    fn set_boundary(&'a mut self, boundary: String) {
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

