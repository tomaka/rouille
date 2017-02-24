// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! Utilities for saving request entries to the filesystem.

use super::buf_redux::copy_buf;

use mime::Mime;

use super::field::{MultipartData, MultipartFile};
use super::Multipart;

use self::SaveResult::*;

pub use tempdir::TempDir;

use std::collections::HashMap;
use std::io::prelude::*;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::{env, fs, io, mem};

const RANDOM_FILENAME_LEN: usize = 12;

// Because this isn't exposed as a str in the stdlib
#[cfg(not(windows))]
const PATH_SEP: &'static str = "/";
#[cfg(windows)]
const PATH_SEP: &'static str = "\\";

fn rand_filename() -> String {
    ::random_alphanumeric(RANDOM_FILENAME_LEN)
}

macro_rules! try_start (
    ($try:expr) => (
        match $try {
            Ok(val) => val,
            Err(e) => return SaveResult::Error(e),
        }
    )
);

macro_rules! try_partial (
    ($try:expr; $partial:expr) => (
        match $try {
            Ok(val) => val,
            Err(e) => Partial($partial, e.into()),
        }
    )
);

/// A builder for saving a file or files to the local filesystem.
///
/// ### `OpenOptions`
/// This builder holds an instance of `std::fs::OpenOptions` which is used
/// when creating the new file(s).
///
/// By default, the open options are set with `.write(true).create_new(true)`,
/// so if the file already exists then an error will be thrown. This is to avoid accidentally
/// overwriting files from other requests.
///
/// If you want to modify the options used to open the save file, you can use
/// `mod_open_opts()`.
///
/// ### File Size and Count Limits
/// You can set a size limit for individual files with `limit()`, which takes either `u64`
/// or `Option<u64>`.
///
/// You can also set the maximum number of files to process with `count_limit()`, which
/// takes either `u32` or `Option<u32>`. This only has an effect when using
/// `SaveBuilder<Multipart>`.
///
/// ### Warning: Do **not* trust user input!
/// It is a serious security risk to create files or directories with paths based on user input.
/// A malicious user could craft a path which can be used to overwrite important files, such as
/// web templates, static assets, Javascript files, database files, configuration files, etc.,
/// if they are writable by the server process.
///
/// This can be mitigated somewhat by setting filesystem permissions as
/// conservatively as possible and running the server under its own user with restricted
/// permissions, but you should still not use user input directly as filesystem paths.
/// If it is truly necessary, you should sanitize user input such that it cannot cause a path to be
/// misinterpreted by the OS. Such functionality is outside the scope of this crate.
#[must_use = "nothing saved to the filesystem yet"]
pub struct SaveBuilder<'s, S: 's> {
    savable: &'s mut S,
    open_opts: OpenOptions,
    limit: Option<u64>,
    count_limit: Option<u32>,
}

impl<'s, S: 's> SaveBuilder<'s, S> {
    /// Implementation detail but not problematic to have accessible.
    #[doc(hidden)]
    pub fn new(savable: &'s mut S) -> SaveBuilder<'s, S> {
        let mut open_opts = OpenOptions::new();
        open_opts.write(true).create_new(true);

        SaveBuilder {
            savable: savable,
            open_opts: open_opts,
            limit: None,
            count_limit: None,
        }
    }

    /// Set the maximum number of bytes to write out *per file*.
    ///
    /// Can be `u64` or `Option<u64>`. If `None`, clears the limit.
    pub fn limit<L: Into<Option<u64>>>(&mut self, limit: L) -> &mut Self {
        self.limit = limit.into();
        self
    }

    /// Modify the `OpenOptions` used to open any files for writing.
    ///
    /// The `write` flag will be reset to `true` after the closure returns. (It'd be pretty
    /// pointless otherwise, right?)
    pub fn mod_open_opts<F: FnOnce(&mut OpenOptions)>(&mut self, opts_fn: F) -> &mut Self {
        opts_fn(&mut self.open_opts);
        self.open_opts.write(true);
        self
    }
}

impl<'s, R: 's> SaveBuilder<'s, Multipart<R>> where R: Read {
    /// Set the maximum number of files to write out.
    ///
    /// Can be `u32` or `Option<u32>`. If `None`, clears the limit.
    pub fn count_limit<L: Into<Option<u32>>>(&mut self, count_limit: L) -> &mut Self {
        self.count_limit = count_limit.into();
        self
    }

    /// Save the file fields in the request to a new temporary directory prefixed with
    /// "multipart-rs" in the OS temporary directory.
    ///
    /// For more options, create a `TempDir` yourself and pass it to `with_temp_dir()` instead.
    ///
    /// ### Note: Temporary
    /// See `SaveDir` for more info (the type of `Entries::save_dir`).
    pub fn temp(&mut self) -> EntriesSaveResult<'s, R> {
        self.temp_with_prefix("multipart-rs")
    }

    /// Save the file fields in the request to a new temporary directory with the given string
    /// as a prefix in the OS temporary directory.
    ///
    /// For more options, create a `TempDir` yourself and pass it to `with_temp_dir()` instead.
    ///
    /// ### Note: Temporary
    /// See `SaveDir` for more info (the type of `Entries::save_dir`).
    pub fn temp_with_prefix(&mut self, prefix: &str) -> EntriesSaveResult<'s, R> {
        match TempDir::new(prefix) {
            Ok(tempdir) => self.with_temp_dir(tempdir),
            Err(e) => SaveResult::Error(e),
        }
    }

    /// Save the file fields to the given `TempDir`.
    ///
    /// The `TempDir` is returned in the result under `Entries::save_dir`.
    pub fn with_temp_dir(&mut self, tempdir: TempDir) -> EntriesSaveResult<'s, R> {
        self.with_entries(Entries::new(SaveDir::Temp(tempdir)))
    }

    /// Save the file fields in the request to a new permanent directory with the given path.
    ///
    /// Any nonexistent parent directories will be created.
    pub fn with_dir<P: Into<PathBuf>>(&mut self, dir: P) -> EntriesSaveResult<'s, R> {
        let dir = dir.into();

        try_start!(create_dir_all(&dir));

        self.with_entries(Entries::new(SaveDir::Perm(dir.into())))
    }

    pub fn with_entries(&mut self, mut entries: Entries) -> EntriesSaveResult<'s, R> {
        let mut count = 0;

        loop {
            let field = match try_partial!(self.savable.read_entry(); PartialEntries {
                entries: entries,
                partial_field: None,
            }) {
                Some(field) => field,
                None => break,
            };

            match field.data {
                MultipartData::File(mut file) => {
                    match self.count_limit {
                        Some(limit) if count >= limit => return SaveResult::Partial (
                            PartialEntries {
                                entries: entries,
                                partial_field: Some(PartialFileField {
                                    field_name: field.name,
                                    file: PartialFile::just_file(file)
                                })
                            },
                            PartialReason::CountLimit,
                        ),
                        _ => (),
                    }

                    count += 1;

                    match file.save().limit(self.limit).with_dir(&entries.save_dir) {
                        Full(saved_file) => entries.mut_files_for(&field.name).push(saved_file),
                        Partial(partial, reason) => return Partial(
                            PartialEntries {
                                entries: entries,
                                partial_field: Some(PartialFileField {
                                    field_name: field.name,
                                    file: partial.with_file(file),
                                })
                            }
                        ),
                        Err(e) => return Partial(
                            PartialEntries {
                                entries: entries,
                                partial_field: Some(PartialFileField {
                                    field_name: field.name,
                                    file: PartialFile::just_file(file)
                                }),
                            },
                            e.into(),
                        ),
                    }
                },
                MultipartData::Text(text) => {
                    entries.fields.insert(field.name, text.text);
                },
            }
        }

        SaveResult::Full(entries)
    }
}

impl<'s, M: 's> SaveBuilder<'s, MultipartFile<M>> where MultipartFile<M>: BufRead {

    /// Save to a file with a random alphanumeric name in the OS temporary directory.
    ///
    /// Does not use user input to create the path.
    ///
    /// See `with_path()` for more details.
    pub fn temp(&mut self) -> FileSaveResult {
        let path = env::temp_dir().join(rand_filename());
        self.with_path(path)
    }

    /// Save to a file with the given name in the OS temporary directory.
    ///
    /// See `with_path()` for more details.
    ///
    /// ### Warning: Do **not* trust user input!
    /// It is a serious security risk to create files or directories with paths based on user input.
    /// A malicious user could craft a path which can be used to overwrite important files, such as
    /// web templates, static assets, Javascript files, database files, configuration files, etc.,
    /// if they are writable by the server process.
    ///
    /// This can be mitigated somewhat by setting filesystem permissions as
    /// conservatively as possible and running the server under its own user with restricted
    /// permissions, but you should still not use user input directly as filesystem paths.
    /// If it is truly necessary, you should sanitize filenames such that they cannot be
    /// misinterpreted by the OS.
    pub fn with_filename(&mut self, filename: &str) -> FileSaveResult {
        let mut tempdir = env::temp_dir();
        tempdir.set_file_name(filename);

        self.with_path(tempdir)
    }

    /// Save to a file with a random alphanumeric name in the given directory.
    ///
    /// See `with_path()` for more details.
    ///
    /// ### Warning: Do **not* trust user input!
    /// It is a serious security risk to create files or directories with paths based on user input.
    /// A malicious user could craft a path which can be used to overwrite important files, such as
    /// web templates, static assets, Javascript files, database files, configuration files, etc.,
    /// if they are writable by the server process.
    ///
    /// This can be mitigated somewhat by setting filesystem permissions as
    /// conservatively as possible and running the server under its own user with restricted
    /// permissions, but you should still not use user input directly as filesystem paths.
    /// If it is truly necessary, you should sanitize filenames such that they cannot be
    /// misinterpreted by the OS.
    pub fn with_dir<P: AsRef<Path>>(&mut self, dir: P) -> FileSaveResult {
        let path = dir.as_ref().join(rand_filename());
        self.with_path(path)
    }

    /// Save to a file with the given path.
    ///
    /// Creates any missing directories in the path.
    /// Uses the contained `OpenOptions` to create the file.
    /// Truncates the file to the given limit, if set.
    pub fn with_path<P: Into<PathBuf>>(&mut self, path: P) -> FileSaveResult {
        let path = path.into();

        let file = match create_dir_all(&path).and_then(|_| self.open_opts.open(&path)) {
            Ok(file) => file,
            Err(e) => return Partial(
                PartialFile {
                    file_path: Some(path),
                    file: (),
                    written: 0,
                    __priv: (),
                },
                e.into(),
            )
        };

        self.write_to(file)
    }


    /// Write out the file field to `dest`, truncating if a limit was set.
    ///
    /// Returns the number of bytes copied, and whether or not the limit was reached
    /// (tested by `MultipartFile::fill_buf().is_empty()` so no bytes are consumed).
    ///
    /// Retries on interrupts.
    pub fn write_to<W: Write>(&mut self, mut dest: W) -> SaveResult<u64, u64> {
        if let Some(limit) = self.limit {
            let copied = match try_copy_buf(self.savable.take(limit), &mut dest) {
                Full(copied) => copied,
                other => return other,
            };

            // If there's more data to be read, the field was truncated
            match self.savable.fill_buf() {
                Ok(ref buf) if buf.is_empty() => Full(copied),
                Ok(_) => Partial(copied, PartialReason::SizeLimit),
                Err(e) => Partial(copied, PartialReason::IoError((e)))
            }
        } else {
            try_copy_buf(&mut self.savable, &mut dest)
        }
    }
}

/// A file saved to the local filesystem from a multipart request.
#[derive(Debug)]
pub struct SavedFile {
    /// The complete path this file was saved at.
    pub path: PathBuf,

    /// ### Warning: Client Provided / Untrustworthy
    /// You should treat this value as **untrustworthy** because it is an arbitrary string
    /// provided by the client.
    ///
    /// It is a serious security risk to create files or directories with paths based on user input.
    /// A malicious user could craft a path which can be used to overwrite important files, such as
    /// web templates, static assets, Javascript files, database files, configuration files, etc.,
    /// if they are writable by the server process.
    ///
    /// This can be mitigated somewhat by setting filesystem permissions as
    /// conservatively as possible and running the server under its own user with restricted
    /// permissions, but you should still not use user input directly as filesystem paths.
    /// If it is truly necessary, you should sanitize filenames such that they cannot be
    /// misinterpreted by the OS. Such functionality is outside the scope of this crate.
    pub filename: Option<String>,

    /// The MIME type (`Content-Type` value) of this file, if supplied by the client,
    /// or `"applicaton/octet-stream"` otherwise.
    ///
    /// ### Note: Client Provided
    /// Consider this value to be potentially untrustworthy, as it is provided by the client.
    /// It may be inaccurate or entirely wrong, depending on how the client determined it.
    ///
    /// Some variants wrap arbitrary strings which could be abused by a malicious user if your
    /// application performs any non-idempotent operations based on their value, such as
    /// starting another program or querying/updating a database (web-search "SQL injection").
    pub content_type: Mime,

    /// The number of bytes written to the disk.
    pub size: u64,
}

/// A result of `Multipart::save_all()`.
#[derive(Debug)]
pub struct Entries {
    /// The text fields of the multipart request, mapped by field name -> value.
    pub fields: HashMap<String, String>,
    /// A map of file field names to their contents saved on the filesystem.
    pub files: HashMap<String, Vec<SavedFile>>,
    /// The directory the files in this request were saved under; may be temporary or permanent.
    pub save_dir: SaveDir,
}

impl Entries {
    fn new(save_dir: SaveDir) -> Self {
        Entries {
            fields: HashMap::new(),
            files: HashMap::new(),
            save_dir: save_dir,
        }
    }

    pub fn mut_files_for(&mut self, field: &str) -> &mut Vec<SavedFile> {
        self.files.entry(field).or_insert_with(Vec::new)
    }
}

/// The save directory for `Entries`. May be temporary (delete-on-drop) or permanent.
#[derive(Debug)]
pub enum SaveDir {
    /// This directory is temporary and will be deleted, along with its contents, when this wrapper
    /// is dropped.
    Temp(TempDir),
    /// This directory is permanent and will be left on the filesystem when this wrapper is dropped.
    Perm(PathBuf),
}

impl SaveDir {
    /// Get the path of this directory, either temporary or permanent.
    pub fn as_path(&self) -> &Path {
        use self::SaveDir::*;
        match *self {
            Temp(ref tempdir) => tempdir.path(),
            Perm(ref pathbuf) => &*pathbuf,
        }
    }

    /// Returns `true` if this is a temporary directory which will be deleted on-drop.
    pub fn is_temporary(&self) -> bool {
        use self::SaveDir::*;
        match *self {
            Temp(_) => true,
            Perm(_) => false,
        }
    }

    /// Unwrap the `PathBuf` from `self`; if this is a temporary directory,
    /// it will be converted to a permanent one.
    pub fn into_path(self) -> PathBuf {
        use self::SaveDir::*;

        match self {
            Temp(tempdir) => tempdir.into_path(),
            Perm(pathbuf) => pathbuf,
        }
    }

    /// If this `SaveDir` is temporary, convert it to permanent.
    /// This is a no-op if it already is permanent.
    ///
    /// ### Warning: Potential Data Loss
    /// Even though this will prevent deletion on-drop, the temporary folder on most OSes
    /// (where this directory is created by default) can be automatically cleared by the OS at any
    /// time, usually on reboot or when free space is low.
    ///
    /// It is recommended that you relocate the files from a request which you want to keep to a
    /// permanent folder on the filesystem.
    pub fn keep(&mut self) {
        use self::SaveDir::*;
        *self = match mem::replace(self, Perm(PathBuf::new())) {
            Temp(tempdir) => Perm(tempdir.into_path()),
            old_self => old_self,
        };
    }

    /// Delete this directory and its contents, regardless of its permanence.
    ///
    /// ### Warning: Potential Data Loss
    /// This is very likely irreversible, depending on the OS implementation.
    ///
    /// Files deleted programmatically are deleted directly from disk, as compared to most file
    /// manager applications which use a staging area from which deleted files can be safely
    /// recovered (i.e. Windows' Recycle Bin, OS X's Trash Can, etc.).
    pub fn delete(self) -> io::Result<()> {
        use self::SaveDir::*;
        match self {
            Temp(tempdir) => tempdir.close(),
            Perm(pathbuf) => fs::remove_dir_all(&pathbuf),
        }
    }
}

impl AsRef<Path> for SaveDir {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// The file that was to be read next when the limit was hit.
#[derive(Clone, Debug)]
pub struct PartialFile<F> {
    /// The path of the file on the filesystem.
    ///
    /// If an error occurred while creating the file, this will not exist.
    pub file_path: Option<PathBuf>,

    /// The file in the multipart stream.
    pub file: F,

    /// The number of bytes written to the filesystem.
    pub written: u64,

    __priv: (),
}

impl<F> PartialFile<F> {
    fn with_file<F_>(self, file: F_) -> PartialFile<F_> {
        PartialFile {
            file_path: self.file_path,
            file: file,
            written: self.written,
            __priv: ()
        }
    }

    fn just_file(file: F) -> Self {
        PartialFile {
            file_path: None,
            file: file,
            written: 0,
            __priv: ()
        }
    }
}

/// The reason the save operation quit partway through.
#[derive(Debug, Clone)]
pub enum PartialReason {
    /// The count limit for files in the request was hit.
    ///
    /// The associated file has not been saved to the filesystem.
    CountLimit,
    /// The size limit for an individual file was hit.
    ///
    /// The file was partially written to the filesystem.
    SizeLimit,
    /// An error occurred during the operation.
    IoError(io::Error),
}

impl From<io::Error> for PartialReason {
    fn from(e: io::Error) -> Self {
        PartialReason::IoError(e)
    }
}

pub struct PartialFileField<'m, R: 'm> {
    /// The field name for the errored file.
    pub field_name: String,
    pub file: PartialFile<MultipartFile<&'m mut Multipart<R>>>,

}

pub struct PartialEntries<'m, R: 'm> {
    pub entries: Entries,
    pub partial_field: Option<PartialFileField<'m, R>>,
}

/// The result of [`Multipart::save_all()`](struct.multipart.html#method.save_all)
/// and methods on `SaveBuilder<Multipart>`.
#[derive(Debug)]
pub enum SaveResult<Success, Partial> {
    /// The operation was a total success. Contained is the complete result.
    Full(Success),
    /// The operation quit partway through. Included is the partial
    /// result along with the reason.
    Partial(Partial, PartialReason),
    /// An error occurred at the start of the operation, before anything was done.
    Error(io::Error),
}

/// Shorthand result for methods that return `Entries`
pub type EntriesSaveResult<'m, R> = SaveResult<Entries, PartialEntries<'m, R>>;

/// Shorthand result for methods that return `SavedFile`s.
///
/// The `MultipartFile` is not available here because it is not necessary to return
/// a borrow when the owned version is probably in the same scope.
pub type FileSaveResult = SaveResult<SavedFile, PartialFile<()>>;

impl<'m, R> EntriesSaveResult<'m, R> {
    /// Take the `Entries` from `self`, if applicable, and discarding
    /// the error, if any.
    pub fn to_entries(self) -> Option<Entries> {
        match self {
            Full(entries) | Partial(PartialEntries { entries, .. }, _) => Some(entries),
            Error(_) => None,
        }
    }

    /// Decompose `self` to `(Option<Entries>, Option<io::Error>)`
    pub fn to_opt(self) -> (Option<Entries>, Option<io::Error>) {
        match self {
            Partial(PartialEntries { entries, .. }, PartialReason::IoError(e)) => (Some(entries), Some(e)),
            Full(entries) | Partial(entries, _) => (Some(entries), None),
            Error(error) => (None, Some(error)),
        }
    }

    /// Map `self` to an `io::Result`, discarding the error in the `Partial` case.
    pub fn to_result(self) -> io::Result<Entries> {
        match self {
            Full(entries) | Partial(PartialEntries { entries, .. }, _) => Ok(entries),
            Error(error) => Err(error),
        }
    }
}

fn create_dir_all(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
    } else {
        // RFC: return an error instead?
        warn!("Attempting to save file in what looks like a root directory. File path: {:?}", path);
        Ok(())
    }
}

fn try_copy_buf<R: BufRead, W: Write>(mut src: R, dest: W) -> SaveResult<u64, u64> {
    let mut total_copied = 0;

    macro_rules! try_here (
        ($try:expr) => (
            match $try {
                Ok(val) => val,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return if total_copied == 0 { Error(e) }
                                 else { Partial(total_copied, e.into()) },
            }
        )
    );

    loop {
        let mut buf = try_here!(src.fill_buf());

        while !buf.is_empty() {
            match try_here!(dest.write(buf)) {
                0 => try_here!(Err(io::Error::new(io::ErrorKind::WriteZero,
                                          "failed to write whole buffer"))),
                copied => {
                    buf = &mut buf[copied..];
                    total_copied += copied as u64;
                    src.consume(copied)
                },
            }
        }
    }

    Full(total_copied)
}
