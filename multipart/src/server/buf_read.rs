use std::cmp;
use std::fmt;

use std::io::prelude::*;
use std::io;

const DEFAULT_BUF_SIZE: usize = 64 * 1024;

/// A copy of `std::io::BufReader` with additional methods needed for multipart support.
pub struct CustomBufReader<R> {
    inner: R,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<R: Read> CustomBufReader<R> { 
    #[doc(hidden)]
    pub fn new(inner: R) -> Self {
        CustomBufReader::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    #[doc(hidden)]
    pub fn with_capacity(cap: usize, inner: R) -> Self {
        CustomBufReader {
            inner: inner,
            buf: vec![0; cap],
            pos: 0,
            cap: 0,
        }
    }

    #[doc(hidden)]
    pub fn fill_buf_min(&mut self, min: usize) -> io::Result<&[u8]> {
        if self.pos == self.cap {
            self.cap = try!(self.inner.read(&mut self.buf));
            self.pos = 0;
        } else if min > self.cap - self.pos {
            self.cap += try!(self.inner.read(&mut self.buf[self.cap..]));
        }            

        Ok(&self.buf[self.pos..self.cap])
    }

    #[doc(hidden)]
    pub fn get_ref(&self) -> &R { &self.inner }
}

impl<R: Read> Read for CustomBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.len() >= self.buf.len() {
            return self.inner.read(buf);
        }
        let nread = {
            let mut rem = try!(self.fill_buf());
            try!(rem.read(buf))
        };
        self.consume(nread);
        Ok(nread)
    }
}

impl<R: Read> BufRead for CustomBufReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the underlying reader.
        if self.pos == self.cap {
            self.cap = try!(self.inner.read(&mut self.buf));
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.cap])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = cmp::min(self.pos + amt, self.cap);
    }
}

impl<R> fmt::Debug for CustomBufReader<R> where R: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_fmt(format_args!(
            "CustomBufReader {{ reader: {:?}, buffer: {}/{}}}", 
            &self.inner, self.cap - self.pos, self.buf.len()
        ))

        /* FIXME (07/26/15): Switch to this impl after the next Stable release.
        fmt.debug_struct("CustomBufReader")
            .field("reader", &self.inner)
            .field("buffer", &format_args!("{}/{}", ))
            .finish()
        */
    }
}
