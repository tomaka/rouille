use std::cmp;
use std::borrow::Borrow;

use std::io;
use std::io::BufReader;
use std::io::prelude::*;

use std::ptr;

/// A struct implementing `Read` that will yield bytes until it sees a given sequence.
#[derive(Debug)]
pub struct BoundaryReader<R> {
    buffer: BufReader<R>,
    boundary: Vec<u8>,
    search_idx: usize,
    boundary_read: bool,
}


impl<R> BoundaryReader<R> where R: Read {
    #[doc(hidden)]
    pub fn from_reader<B: Into<Vec<u8>>>(reader: R, boundary: B) -> BoundaryReader<R> {
        BoundaryReader {
            buffer: BufReader::new(reader),
            boundary: boundary.into(),
            search_idx: 0,
            boundary_read: false,
        }
    }

    fn read_to_boundary(&mut self) -> io::Result<&[u8]> {
        let buf = try!(self.buffer.fill_buf());

        if !self.boundary_read {
            let boundary_0 = self.boundary[0];

            let lookahead_iter = buf[self.search_idx..].windows(self.boundary.len()).enumerate();

            for (search_idx, maybe_boundary) in lookahead_iter {
                if maybe_boundary[0] == self.boundary[0] {
                    self.boundary_read = self.boundary == maybe_boundary;
                    self.search_idx = search_idx;

                    if self.boundary_read {
                        break;
                    }
                }
            }
        }
        debug!("Buf len: {} Search idx: {}", buf.len(), self.search_idx);
        Ok(&buf[..self.search_idx]) 
    }

    #[doc(hidden)]
    pub fn consume_boundary(&mut self) -> io::Result<()> {
        while !self.boundary_read {
            let buf_len = try!(self.read_to_boundary()).len();
            self.consume(buf_len);
        }

        let consume_amt = {
            let boundary_len = self.boundary.len();
            let buf = try!(self.read_to_boundary());
            buf.len() + boundary_len
        };

        self.buffer.consume(consume_amt);
        self.search_idx = 0;
        self.boundary_read = false;
Ok(())
    }

    #[allow(unused)]
    #[doc(hidden)]
    pub fn set_boundary<B: Into<Vec<u8>>>(&mut self, boundary: B) {
        self.boundary = boundary.into();
    }
}

impl<R: Read> Borrow<R> for BoundaryReader<R> {
    fn borrow(&self) -> &R {
        self.buffer.get_ref() 
    }
}

impl<R> Read for BoundaryReader<R> where R: Read {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let consume_len = {
            let buf = try!(self.read_to_boundary());
            let trunc_len = cmp::min(buf.len(), out.len());
            copy_bytes(&buf[..trunc_len], out);
            trunc_len
        };

        self.consume(consume_len);

        Ok(consume_len)
    }
}

impl<R> BufRead for BoundaryReader<R> where R: Read {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.read_to_boundary()
    }

    fn consume(&mut self, amt: usize) {
        let true_amt = cmp::min(amt, self.search_idx);
        self.buffer.consume(true_amt);
        self.search_idx -= true_amt;
    }
}

// copied from `std::slice::bytes` due to unstable
fn copy_bytes(src: &[u8], dst: &mut [u8]) {
    let len_src = src.len();
    assert!(dst.len() >= len_src);
    // `dst` is unaliasable, so we know statically it doesn't overlap with `src`.
    unsafe {
        ptr::copy_nonoverlapping(
            src.as_ptr(),
            dst.as_mut_ptr(),
            len_src
        );
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

    ::env_logger::init().unwrap();

    let src = &mut TEST_VAL.as_bytes();
    let mut reader = BoundaryReader::from_reader(src, BOUNDARY);

    let ref mut buf = String::new();

    debug!("Read 1");
    let _ = reader.read_to_string(buf).unwrap();
    debug!("Buf: {:?}", buf);
    assert!(buf.trim().is_empty());

    buf.clear();

    debug!("Consume 1");
    reader.consume_boundary().unwrap();

    debug!("Read 2");
    let _ = reader.read_to_string(buf).unwrap();
    assert_eq!(buf.trim(), "dashed-value-1");
    buf.clear();

    debug!("Consume 2");
    reader.consume_boundary().unwrap();

    debug!("Read 3");
    let _ = reader.read_to_string(buf).unwrap();
    assert_eq!(buf.trim(), "dashed-value-2");

    debug!("Consume 3");
    reader.consume_boundary().unwrap();
}
