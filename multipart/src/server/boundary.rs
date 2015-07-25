use server::buf_read::CustomBufReader;

use std::cmp;
use std::borrow::Borrow;

use std::io;
use std::io::prelude::*;

use std::ptr;

/// A struct implementing `Read` and `BufRead` that will yield bytes until it sees a given sequence.
#[derive(Debug)]
pub struct BoundaryReader<R> {
    buffer: CustomBufReader<R>,
    boundary: Vec<u8>,
    search_idx: usize,
    boundary_read: bool,
    at_end: bool,
}

impl<R> BoundaryReader<R> where R: Read {
    #[doc(hidden)]
    pub fn from_reader<B: Into<Vec<u8>>>(reader: R, boundary: B) -> BoundaryReader<R> {
        BoundaryReader {
            buffer: CustomBufReader::new(reader),
            boundary: boundary.into(),
            search_idx: 0,
            boundary_read: false,
            at_end: false,
        }
    }

    fn read_to_boundary(&mut self) -> io::Result<&[u8]> {
        use log::LogLevel;

        let buf = try!(self.buffer.fill_buf_min(self.boundary.len()));

        while !(self.boundary_read || self.at_end) && self.search_idx < buf.len() {
            let lookahead = &buf[self.search_idx..];

            let safe_len = cmp::min(lookahead.len(), self.boundary.len());

            if &lookahead[..safe_len] == &self.boundary[..safe_len] {
                self.boundary_read = safe_len == self.boundary.len();
                break;
            }

            self.search_idx += 1;
        }

        debug!(
            "Buf len: {} Search idx: {} Boundary read: {:?}", 
            buf.len(), self.search_idx, self.boundary_read
        );
        
        if log_enabled!(LogLevel::Info) {
            let _ = ::std::str::from_utf8(buf).map(|buf|
                info!("Buf: {:?}", buf)
            );
        }

        if self.search_idx >= 2 {
            Ok(&buf[..self.search_idx - 2])
        } else {
            Ok(&buf[..self.search_idx])
        }
    }

    #[doc(hidden)]
    pub fn consume_boundary(&mut self) -> io::Result<()> {
        while !self.boundary_read {
            let buf_len = try!(self.read_to_boundary()).len();

            if buf_len == 0 {
                break;
            }

            self.consume(buf_len);
        }
        

        let consume_amt = {
            let mut buf = try!(self.buffer.fill_buf_min(self.boundary.len() + 4));
            let mut consume_amt = self.boundary.len() + 2;

            if self.search_idx != 0 {
                let (before, new_buf) = buf.split_at(2);
                if before != b"\r\n" {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Expected {:?}, got {:?}", b"\r\n", &before)
                    ));
                }

                buf = new_buf;
                consume_amt += 2;
            }

            let (boundary, after) = buf.split_at(self.boundary.len());
            if &*self.boundary != boundary {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Expected {:?}, got {:?}", self.boundary, boundary)
                ));
            }

            if &after[..2] != b"\r\n" {
                self.at_end = after == b"--";

                if !self.at_end {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "Expected {:?} or {:?}, got {:?}", 
                                b"\r\n", b"--", after
                        )
                    ));
                }
            }

            consume_amt
        };

        self.buffer.consume(consume_amt);

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
        let mut search_idx = self.search_idx;

        if search_idx >= 2 {
            search_idx -= 2;
        }

        let true_amt = cmp::min(amt, search_idx);
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

#[cfg(test)]
mod test {
    use super::BoundaryReader;

    use std::cmp;

    use std::io;
    use std::io::prelude::*;

    const BOUNDARY: &'static str = "--boundary";
    const TEST_VAL: &'static str = "--boundary\r
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
                let copy_amt = cmp::min(src.len(), dst.len());
                super::copy_bytes(&src[..copy_amt], dst);
                *src = &src[copy_amt..];
                copy_amt
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
        assert_eq!(buf, "dashed-value-1");
        buf.clear();

        debug!("Consume 2");
        reader.consume_boundary().unwrap();

        debug!("Read 3");
        let _ = reader.read_to_string(buf).unwrap();
        assert_eq!(buf, "dashed-value-2");
        buf.clear();

        debug!("Consume 3");
        reader.consume_boundary().unwrap();

        debug!("Read 4");
        let _ = reader.read_to_string(buf).unwrap();
        assert!(buf.is_empty(), "Buffer not empty: {:?}", buf);        
    }
}
