use hyper::header::common::content_type::ContentType;
use hyper::server::request::Request;

use mime::{Mime, AttrExt, ValueExt};

use super::MultipartField;

use std::io::{RefReader, BufferedReader, IoError, IoResult, EndOfFile, standard_error, OtherIoError};

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

impl Iterator<(String, MultipartField)> for Multipart {
    fn next(&mut self) -> Option<(String, MultipartField)> {
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

