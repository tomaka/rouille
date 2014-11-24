use hyper::header::common::content_type::ContentType;
use hyper::server::request::Request;

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use super::{MultipartField, MultipartFile};

use std::cmp;

use std::io::{IoError, IoResult, EndOfFile, standard_error, OtherIoError};

fn is_multipart_formdata(req: &Request) -> bool {
    use mime::{Multipart};

    req.headers.get::<ContentType>().map_or(false, |ct| {
        let ContentType(ref mime) = *ct;
        
        debug!("Content-Type: {}", mime);

        match *mime {
            Mime(TopLevel::Multipart, SubLevel::FormData, _) => true,
            _ => false,   
        }
    })
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

pub struct Multipart<'a> {
    source: BoundaryReader<Request<'a>>,
}

macro_rules! try_find(
    ($haystack:expr, $f:ident, $needle:expr, $err:expr, $line:expr) => (
        try!($haystack.$f($needle).ok_or(line_error($err, $line.clone())))
    )
)

impl<'a> Multipart<'a> {

    /// If the given `Request` is of `Content-Type: multipart/form-data`, return
    /// the wrapped request as `Ok(Multipart)`, otherwise `Err(Request)`.
    pub fn from_request(req: Request<'a>) -> Result<Multipart<'a>, Request<'a>> {
        if !is_multipart_formdata(&req) { return Err(req); }

        debug!("Is multipart!");

        let boundary = if let Some(boundary) = req.headers.get::<ContentType>()
            .and_then(get_boundary) { boundary } else { return Err(req); };

        debug!("Boundary: {}", boundary);

        Ok(Multipart { source: BoundaryReader::from_reader(req, format!("--{}\r\n", boundary)) })
    }

    pub fn read_entry<'a>(&'a mut self) -> IoResult<(String, MultipartField<'a>)> {
        debug!("Read entry!");
 
        try!(self.source.consume_boundary());
        let (disp_type, field_name, filename) = try!(self.read_content_disposition());

        if &*disp_type != "form-data" {
            return Err(IoError {
                    kind: OtherIoError,
                    desc: "Content-Disposition value was not \"form-data\"",
                    detail: Some(format!("Content-Disposition: {}", disp_type)),
                });
        }
      
        if let Some(content_type) = try!(self.read_content_type()) {
            let _ = try!(self.source.read_line()); // Consume empty line
            Ok((field_name, 
                MultipartField::File(
                    MultipartFile::from_octet(filename, &mut self.source, content_type[])
                )
            ))
        } else {
            // Empty line consumed by read_content_type()
            let text = try!(self.source.read_to_string());
            // The last two characters are "\r\n".
            // We can't do a simple trim because the content might be terminated
            // with line separators we want to preserve.
            Ok((field_name, MultipartField::Text(text[..text.len() - 2].into_string()))) 
        }                
    }
   
    /// Call `f` for each entry in the multipart request.
    /// This is a substitute for `Multipart` implementing `Iterator`,
    /// since `Iterator::next()` can't use bound lifetimes.
    /// See https://www.reddit.com/r/rust/comments/2lkk4i/concrete_lifetime_vs_bound_lifetime/
    pub fn foreach_entry<F: for<'a> FnMut(String, MultipartField<'a>)>(&mut self, mut f: F) {
        loop {
            debug!("Loop!");

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
        debug!("Read content disposition!");
        let line = try!(self.source.read_line());       

        debug!("Line: {}", line);

        // Find the end of CONT_DISP in the line
        let disp_type = {
            const CONT_DISP: &'static str = "Content-Disposition:";

            let disp_idx = try_find!(line[], find_str, CONT_DISP, 
                "Content-Disposition subheader not found!", line) + CONT_DISP.len();
                
            debug!("Disp idx: {} Line len: {}", disp_idx, line.len());  

            let disp_type_end = try_find!(line[disp_idx..], find, ';', 
                "Error parsing Content-Disposition value!", line);

            debug!("Disp end: {}", disp_type_end);

            line[disp_idx .. disp_idx + disp_type_end].trim().into_string()
        };
   
        debug!("Disp-type: {}", disp_type);
    
        let field_name = {
            const NAME: &'static str = "name=\"";

            let name_idx = try_find!(line[], find_str, NAME, 
                "Error parsing field name!", line) + NAME.len();

            debug!("Name idx: {}", name_idx);

            let name_end = try_find!(line[name_idx ..], find, '"',
                "Error parsing field name!", line);

            debug!("Name end: {}", name_end);

            line[name_idx .. name_idx + name_end].into_string() // No trim here since it's in quotes.
        };

        debug!("Field name: {}", field_name);

        let filename = {
            const FILENAME: &'static str = "filename=\"";

            let filename_idx = line[].find_str(FILENAME).map(|idx| idx + FILENAME.len());
            let filename_idxs = with(filename_idx, |&start| line[start ..].find('"'));
            
            filename_idxs.map(|(start, end)| line[start .. start + end].into_string())
        };

        debug!("Filename: {}", filename);
        
        Ok((disp_type, field_name, filename))
    }

    fn read_content_type(&mut self) -> IoResult<Option<String>> {
        debug!("Read content type!");
        let line = try!(self.source.read_line());

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
pub struct BoundaryReader<S> {
    reader: S,
    boundary: Vec<u8>,
    last_search_idx: uint,
    boundary_read: bool,
    buf: Vec<u8>,
    buf_len: uint,
}

fn eof<T>() -> IoResult<T> {
    Err(standard_error(EndOfFile))    
}

const BUF_SIZE: uint = 1024 * 64; // 64k buffer

impl<S> BoundaryReader<S> where S: Reader {
    fn from_reader(reader: S, boundary: String) -> BoundaryReader<S> {
        let mut buf = Vec::with_capacity(BUF_SIZE);
        unsafe { buf.set_len(BUF_SIZE); }

        BoundaryReader {
            reader: reader,
            boundary: boundary.into_bytes(),
            last_search_idx: 0,
            boundary_read: false,
            buf: buf,
            buf_len: 0,  
        }
    }

    fn read_to_boundary(&mut self) -> IoResult<()> {
        debug!("Read to boundary!");

         if !self.boundary_read {
            try!(self.true_fill_buf());

            debug!("Exited true_fill_buf");

            if self.buf_len == 0 { return eof(); }
            
            let lookahead = self.buf[self.last_search_idx .. self.buf_len];

            debug!("Buf len: {}", self.buf_len);

            debug!("Lookahead: {}", lookahead.to_ascii().as_str_ascii());
             
            let search_idx = lookahead.position_elem(&self.boundary[0])
                .unwrap_or(lookahead.len() - 1);

            debug!("Search idx: {}", search_idx);

            self.boundary_read = lookahead[search_idx..]
                .starts_with(self.boundary[]);

            debug!("Boundary read: {} Boundary: {}", self.boundary_read, self.boundary.to_ascii().as_str_ascii());

            self.last_search_idx += search_idx;

            if !self.boundary_read {
                self.last_search_idx += 1;    
            }

        } else if self.last_search_idx == 0 {
            return Err(standard_error(EndOfFile))                
        }
        
        Ok(()) 
    }

    /// Read bytes until the reader is full
    fn true_fill_buf(&mut self) -> IoResult<()> {
        debug!("True fill buf! Buf len: {}", self.buf_len);

        let mut bytes_read = 1u;
        
        while bytes_read != 0 {
            debug!("Bytes read loop!");

            bytes_read = match self.reader.read(self.buf[mut self.buf_len..]) {
                Ok(read) => read,
                Err(err) => if err.kind == EndOfFile { break; } else { return Err(err); },
            };

            debug!("Bytes read: {}", bytes_read);

            self.buf_len += bytes_read;
        }

        debug!("Exited bytes read loop!");

        Ok(())
    }

    fn _consume(&mut self, amt: uint) {
        use std::ptr::copy_memory;

        debug!("Consume! Amt: {}", amt);
        
        assert!(amt <= self.buf_len);

        let src = self.buf[amt..].as_ptr();
        let dest = self.buf[mut].as_mut_ptr();

        unsafe { copy_memory(dest, src, self.buf_len - amt); }
        
        self.buf_len -= amt;
        self.last_search_idx -= amt; 
    }

    fn consume_boundary(&mut self) -> IoResult<()> {
        debug!("Consume boundary!");

        while !self.boundary_read {
            debug!("Boundary read loop!");

            match self.read_to_boundary() {
                Ok(_) => (),
                Err(e) => if e.kind == EndOfFile { 
                    break; 
                } else { 
                    return Err(e);
                }
            }
        }
       
        let consume_amt = cmp::min(self.buf_len, self.last_search_idx + self.boundary.len());

        debug!("Consume amt: {} Buf len: {}", consume_amt, self.buf_len);

        self._consume(consume_amt);
        self.last_search_idx = 0;
        self.boundary_read = false;  
        
        Ok(())  
    }

    #[allow(unused)]
    fn set_boundary(&mut self, boundary: String) {
        self.boundary = boundary.into_bytes();    
    }
}

impl<S> Reader for BoundaryReader<S> where S: Reader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        use std::cmp;        
        use std::slice::bytes::copy_memory;

        debug!("Read!");

        try!(self.read_to_boundary()); 

        let trunc_len = cmp::min(buf.len(), self.last_search_idx);
        copy_memory(buf, self.buf[..trunc_len]); 

        self._consume(trunc_len);

        Ok(trunc_len)        
    } 
}

impl<S> Buffer for BoundaryReader<S> where S: Reader {
    fn fill_buf<'a>(&'a mut self) -> IoResult<&'a [u8]> {
        debug!("Fill buf!");

        try!(self.read_to_boundary());
       
        let buf = self.buf[..self.last_search_idx];
        
        debug!("Buf: {}", buf.to_ascii().as_str_ascii());

        Ok(buf)    
    }

    fn consume(&mut self, amt: uint) {
        assert!(amt <= self.last_search_idx);
        self._consume(amt);
    }
}

#[test]
fn test_boundary() {
    use std::io::BufReader;

    const BOUNDARY: &'static str = "--boundary\r\n";
    const TEST_VAL: &'static str = "\r
--boundary\r
dashed-value-1\r
--boundary\r
dashed-value-2\r
--boundary\r
";

    let test_reader = BufReader::new(TEST_VAL.as_bytes());
    let mut reader = BoundaryReader::from_reader(test_reader, BOUNDARY.into_string());

    debug!("Read 1");
    let string = reader.read_to_string().unwrap();
    debug!("{}", string);
    assert!(string[].trim().is_empty());

    debug!("Consume 1");
    reader.consume_boundary().unwrap();

    debug!("Read 2");
    assert_eq!(reader.read_to_string().unwrap()[].trim(), "dashed-value-1");

    debug!("Consume 2");
    reader.consume_boundary().unwrap();

    debug!("Read 3");
    assert_eq!(reader.read_to_string().unwrap()[].trim(), "dashed-value-2");

    debug!("Consume 3");
    reader.consume_boundary().unwrap();
   
}

