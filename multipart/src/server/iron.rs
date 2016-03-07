//! Adaptor types and impls for `iron::Request`. Includes a `BeforeMiddleware` implementation. 

use iron::headers::ContentType;
use iron::BeforeMiddleware;
use iron::mime::{Mime, TopLevel, SubLevel};
use iron::request::{Body, Request};

use tempdir::TempDir;

use std::collections::HashMap;
use std::path::PathBuf;

use super::{HttpRequest, Multipart};

impl<'r, 'a, 'b> HttpRequest for &'r mut Request<'a, 'b> {
    type Body = &'r mut Body<'a, 'b>;

    fn multipart_boundary(&self) -> Option<&str> {
        let content_type = try_opt!(self.headers.get::<ContentType>());
        if let Mime(TopLevel::Multipart, SubLevel::FormData, _) = *content_type {
            content_type.get_param("boundary").map(|b| b.as_str())
        } else {
            None
        }
    }

    fn body(self) -> &'r mut Body<'a, 'b> {
        &mut self.body
    }
}

/// The default file size limit for `Intercept`, in bytes.
pub const DEFAULT_FILE_SIZE_LIMIT: u64 = 2 * 1024 * 1024;

/// The default file count limit for `Intercept`.
pub const DEFAULT_FILE_COUNT_LIMIT: u64 = 16;

pub struct Intercept {
    pub temp_dir_path: Option<PathBuf>,
    pub file_size_limit: u64,
    pub file_count_limit: u64,
}

impl Intercept {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn temp_dir_path<P: Into<PathBuf>>(self, path: P) -> Self {
        Intercept { temp_dir_path: path.into(), .. self }
    }

    pub fn file_size_limit(self, limit: u64) -> Self {
        Intercept { file_size_limit: limit, .. self }
    }

    pub fn file_count_limit(self, limit: u64) -> Self {
        Intercept { file_count_limit: limit, .. self }
    }
}

impl Default for Intercept {
    fn default() -> Self {
        Intercept {
            temp_dir_path: None,
            file_size_limit: DEFAULT_FILE_SIZE_LIMIT,
            file_count_limit: DEFAULT_FILE_COUNT_LIMIT,
        }
    }
}

impl BeforeMiddleware for Intercept {
    fn before(&self, req: &mut Request) -> IronResult<()> {
         
    }
}

pub enum LimitBehavior {
    ThrowError,
    Continue,
}

struct LimitReader<R> {
    inner: R,
    limit: u64,
}
