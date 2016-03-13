// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
extern crate buf_redux;
extern crate memchr;

use self::buf_redux::BufReader;
use self::memchr::memchr;

use std::cmp;
use std::borrow::Borrow;

use std::io;
use std::io::prelude::*;

/// A struct implementing `Read` and `BufRead` that will yield bytes until it sees a given sequence.
#[derive(Debug)]
pub struct BoundaryReader<R> {
    buf: BufReader<R>,
    boundary: Vec<u8>,
    search_idx: usize,
    boundary_read: bool,
    at_end: bool,
}

impl<R> BoundaryReader<R> where R: Read {
    #[doc(hidden)]
    pub fn from_reader<B: Into<Vec<u8>>>(reader: R, boundary: B) -> BoundaryReader<R> {
        BoundaryReader {
            buf: BufReader::new(reader),
            boundary: boundary.into(),
            search_idx: 0,
            boundary_read: false,
            at_end: false,
        }
    }

    fn read_to_boundary(&mut self) -> io::Result<&[u8]> {
        use log::LogLevel;

        let buf = try!(fill_buf_min(&mut self.buf, self.boundary.len()));
        
        if log_enabled!(LogLevel::Trace) {
            trace!("Buf: {:?}", String::from_utf8_lossy(buf));
        }

        debug!(
            "Before-loop Buf len: {} Search idx: {} Boundary read: {:?}", 
            buf.len(), self.search_idx, self.boundary_read
        );

        while !(self.boundary_read || self.at_end) && self.search_idx < buf.len() {
            let lookahead = &buf[self.search_idx..];

            let maybe_boundary = memchr(self.boundary[0], lookahead);

            debug!("maybe_boundary: {:?}", maybe_boundary);

            self.search_idx = match maybe_boundary {
                Some(boundary_start) => self.search_idx + boundary_start,
                None => buf.len(),
            };

            if self.search_idx + self.boundary.len() <= buf.len() {
                let test = &buf[self.search_idx .. self.search_idx + self.boundary.len()];

                match first_nonmatching_idx(test, &self.boundary) {
                    Some(idx) => self.search_idx += idx,
                    None => self.boundary_read = true,
                } 
            } else {
                break;
            }            
        }        
        
        debug!(
            "After-loop Buf len: {} Search idx: {} Boundary read: {:?}", 
            buf.len(), self.search_idx, self.boundary_read
        );


        let mut buf_end = self.search_idx;
        
        if self.boundary_read && self.search_idx >= 2 {
            let two_bytes_before = &buf[self.search_idx - 2 .. self.search_idx];

            debug!("Two bytes before: {:?} (\"\\r\\n\": {:?})", two_bytes_before, b"\r\n");

            if two_bytes_before == &*b"\r\n" {
                debug!("Subtract two!");
                buf_end -= 2;
            } 
        }

        let ret_buf = &buf[..buf_end];

        if log_enabled!(LogLevel::Trace) {
            trace!("Returning buf: {:?}", String::from_utf8_lossy(ret_buf));
        }

        Ok(ret_buf)
    }

    #[doc(hidden)]
    pub fn consume_boundary(&mut self) -> io::Result<()> {
        if self.at_end {
            return Ok(());
        }

        while !self.boundary_read {
            let buf_len = try!(self.read_to_boundary()).len();

            if buf_len == 0 {
                break;
            }

            self.consume(buf_len);
        }

        self.buf.consume(self.search_idx + self.boundary.len());

        self.search_idx = 0;
        self.boundary_read = false;
 
        Ok(())
    }

    // Keeping this around to support nested boundaries later.
    #[allow(unused)]
    #[doc(hidden)]
    pub fn set_boundary<B: Into<Vec<u8>>>(&mut self, boundary: B) {
        self.boundary = boundary.into();
    }
}

impl<R> Borrow<R> for BoundaryReader<R> {
    fn borrow(&self) -> &R {
        self.buf.get_ref() 
    }
}

impl<R> Read for BoundaryReader<R> where R: Read {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let read = {
            let mut buf = try!(self.read_to_boundary());
            // This shouldn't ever be an error so unwrapping is fine.
            buf.read(out).unwrap()
        };

        self.consume(read);
        Ok(read)
    }
}

impl<R> BufRead for BoundaryReader<R> where R: Read {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.read_to_boundary()
    }

    fn consume(&mut self, amt: usize) {
        let true_amt = cmp::min(amt, self.search_idx);

        debug!("Consume! amt: {} true amt: {}", amt, true_amt);

        self.buf.consume(true_amt);
        self.search_idx -= true_amt;
    }
}

fn fill_buf_min<R: Read>(buf: &mut BufReader<R>, min: usize) -> io::Result<&[u8]> {
    if buf.available() < min {
        try!(buf.read_into_buf());
    }

    Ok(buf.get_buf())
}

fn first_nonmatching_idx(left: &[u8], right: &[u8]) -> Option<usize> {
    for (idx, (lb, rb)) in left.iter().zip(right).enumerate() {
        if lb != rb {
            return Some(idx);
        }
    }

    None
}

#[cfg(test)]
mod test {
    use super::BoundaryReader;

    use std::io;
    use std::io::prelude::*;

    const BOUNDARY: &'static str = "\r\n--boundary";
    const TEST_VAL: &'static str = "\r\n--boundary\r
dashed-value-1\r
--boundary\r
dashed-value-2\r
--boundary--"; 
        
    #[test]
    fn test_boundary() {
        let _ = ::env_logger::init();        
        debug!("Testing boundary (no split)");

        let src = &mut TEST_VAL.as_bytes();
        let reader = BoundaryReader::from_reader(src, BOUNDARY);
        
        test_boundary_reader(reader);        
    }

    struct SplitReader<'a> {
        left: &'a [u8],
        right: &'a [u8],
    }

    impl<'a> SplitReader<'a> {
        fn split(data: &'a [u8], at: usize) -> SplitReader<'a> {
            let (left, right) = data.split_at(at);

            SplitReader { 
                left: left,
                right: right,
            }
        }
    }

    impl<'a> Read for SplitReader<'a> {
        fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
            fn copy_bytes_partial(src: &mut &[u8], dst: &mut [u8]) -> usize {
                src.read(dst).unwrap()
            }

            let mut copy_amt = copy_bytes_partial(&mut self.left, dst);

            if copy_amt == 0 {
                copy_amt = copy_bytes_partial(&mut self.right, dst)
            };

            Ok(copy_amt)
        }
    }

    #[test]
    fn test_split_boundary() {
        let _ = ::env_logger::init();        
        debug!("Testing boundary (split)");
        
        // Substitute for `.step_by()` being unstable.
        for split_at in (0 .. TEST_VAL.len()).filter(|x| x % 2 != 0) {
            debug!("Testing split at: {}", split_at);

            let src = SplitReader::split(TEST_VAL.as_bytes(), split_at);
            let reader = BoundaryReader::from_reader(src, BOUNDARY);
            test_boundary_reader(reader);
        }

    }

    fn test_boundary_reader<R: Read>(mut reader: BoundaryReader<R>) {
        let ref mut buf = String::new();    

        debug!("Read 1");
        let _ = reader.read_to_string(buf).unwrap();
        assert!(buf.is_empty(), "Buffer not empty: {:?}", buf);
        buf.clear();

        debug!("Consume 1");
        reader.consume_boundary().unwrap();

        debug!("Read 2");
        let _ = reader.read_to_string(buf).unwrap();
        assert_eq!(buf, "\r\ndashed-value-1");
        buf.clear();

        debug!("Consume 2");
        reader.consume_boundary().unwrap();

        debug!("Read 3");
        let _ = reader.read_to_string(buf).unwrap();
        assert_eq!(buf, "\r\ndashed-value-2");
        buf.clear();

        debug!("Consume 3");
        reader.consume_boundary().unwrap();

        debug!("Read 4");
        let _ = reader.read_to_string(buf).unwrap();
        assert_eq!(buf, "--");
    }
}
