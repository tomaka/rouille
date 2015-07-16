/*!

The first parameter of a handler must always implement the `Input` trait defined here.

*/
pub use self::ignore::Ignore;

use std::io::Read;

mod ignore;

/// Objects that implement this trait describe the user's input.
pub trait Input {
    /// The error that `process` can return.
    type Err;

    /// Returns true if this input method matches the user content type.
    fn matches_content_type(content_type_header: &str) -> bool;

    /// Should only be called if `matches_content_type` returned true.
    fn process<R>(R) -> Result<Self, Self::Err> where R: Read;
}
