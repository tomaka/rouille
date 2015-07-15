/*!

The first parameter of a handler must always implement the `Input` trait defined here.

*/
pub use self::ignore::Ignore;

use std::io::Read;

mod ignore;

/// Objects that implement this trait describe the user's input.
pub trait Input {
    /// True if this input method matches the user content type.
    fn matches(content_type_header: &str) -> bool;

    /// Should only be called if `matches` returned true.
    fn process<R>(R) -> Self where R: Read;
}
