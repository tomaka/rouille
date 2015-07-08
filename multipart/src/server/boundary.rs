use std::cmp;
use std::borrow::Borrow;

use std::io;
use std::io::prelude::*;

use std::ptr;

/// A struct implementing `Read` that will yield bytes until it sees a given sequence.
#[derive(Debug)]
pub struct BoundaryReader<S> {
    reader: S,
    boundary: Vec<u8>,
    last_search_idx: usize,
    boundary_read: bool,
    buf: Vec<u8>,
    buf_len: usize,
}

const BUF_SIZE: usize = 1024 * 64; // 64k buffer

impl<S> BoundaryReader<S> where S: Read {
    #[doc(hidden)]
    pub fn from_reader(reader: S, boundary: String) -> BoundaryReader<S> {
        let mut buf = vec![0u8; BUF_SIZE];

        BoundaryReader {
            reader: reader,
            boundary: boundary.into_bytes(),
            last_search_idx: 0,
            boundary_read: false,
            buf: buf,
            buf_len: 0,
        }
    }

    fn read_to_boundary(&mut self) -> io::Result<()> {
         if !self.boundary_read {
            try!(self.true_fill_buf());

            if self.buf_len == 0 { return Ok(()); }

            let lookahead = &self.buf[self.last_search_idx .. self.buf_len];

            let search_idx = lookahead.iter().position(|&byte| byte == self.boundary[0])
                .unwrap_or(lookahead.len() - 1);

            debug!("Search idx: {}", search_idx);

            self.boundary_read = lookahead[search_idx..]
                .starts_with(&self.boundary);

            self.last_search_idx += search_idx;

            if !self.boundary_read {
                self.last_search_idx += 1;
            }
        }

        Ok(()) 
    }

    /// Read bytes until the reader is full
    fn true_fill_buf(&mut self) -> io::Result<()> {
        let mut bytes_read = 0;

        loop {
            bytes_read = try!(self.reader.read(&mut self.buf[self.buf_len..]));
            if bytes_read == 0 { break; }
            self.buf_len += bytes_read;
        }

        Ok(())
    }

    fn _consume(&mut self, amt: usize) {
        use std::ptr;

        assert!(amt <= self.buf_len);

        let (dest, src) = self.buf.split_at_mut(amt);

        copy_bytes(src, dest);

        self.buf_len -= amt;
        self.last_search_idx -= amt;
    }

    #[doc(hidden)]
    pub fn consume_boundary(&mut self) -> io::Result<()> {
        while !self.boundary_read {
            try!(self.read_to_boundary()); 
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

impl<R> Borrow<R> for BoundaryReader<R> {
    fn borrow(&self) -> &R {
        &self.reader
    }
}

impl<R> Read for BoundaryReader<R> where R: Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use std::cmp;

        try!(self.read_to_boundary());

        let trunc_len = cmp::min(buf.len(), self.last_search_idx);
        copy_bytes(&self.buf[..trunc_len], buf);

        self._consume(trunc_len);

        Ok(trunc_len)
    }
}

impl<R> BufRead for BoundaryReader<R> where R: Read {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        try!(self.read_to_boundary());

        let buf = &self.buf[..self.last_search_idx];

        Ok(buf)
    }

    fn consume(&mut self, amt: usize) {
        assert!(amt <= self.last_search_idx);
        self._consume(amt);
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

    let test_reader = BufReader::new(TEST_VAL.as_bytes());
    let mut reader = BoundaryReader::from_reader(test_reader, BOUNDARY.to_owned());

    let ref mut buf = String::new();

    debug!("Read 1");
    let _ = reader.read_to_string(buf).unwrap();
    debug!("{}", buf);
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
