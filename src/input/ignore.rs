use super::Input;
use std::io::Read;

/// "Null" input data. Matches any user input. Use this when you don't need user input.
pub struct Ignore;

impl Input for Ignore {
    fn matches_content_type(_: &str) -> bool {
        true
    }

    fn process<R>(_: R) -> Self where R: Read {
        Ignore
    }
}
