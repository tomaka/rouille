// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

#[cfg(feature = "rustc-serialize")]
mod rustc_serialize;
#[cfg(feature = "rustc-serialize")]
pub use self::rustc_serialize::*;

#[cfg(feature = "serdejson")]
mod serde;
#[cfg(feature = "serdejson")]
pub use self::serde::*;
