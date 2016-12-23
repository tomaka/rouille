// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

/// Equivalent to a `match` expression but for routes.
///
/// # Example
///
/// ```no_run
/// # #[macro_use] extern crate rouille; fn main() {
/// # let request: rouille::Request = unsafe { std::mem::uninitialized() };
/// let _result = router!(request,
///     // first route
///     (GET) (/) => {
///         12
///     },
///
///     // second route
///     (GET) (/hello) => {
///         43 * 7
///     },
///
///     // ... other routes here ...
///
///     // default route
///     _ => 5
/// );
/// # }
/// ```
///
/// # Details
///
/// The macro will take each route one by one and execute the first one that matches, similar to a
/// `match` language construct. The whole `router!` expression then returns what the body
/// returns, therefore all the bodies must return the same type of data.
///
/// You can use parameters by putting them inside `{}`:
///
/// ```ignore
/// (GET) (/{id}/foo) => {
///     ...
/// },
/// ```
///
/// If you use parameters inside `{}`, then a variable with the same name will be available in the
/// code in the body.
///
/// Each parameter gets parsed through the `FromStr` trait. If the parsing fails, the route is
/// ignored. If you get an error because the type of the parameter couldn't be inferred, you can
/// also specify the type inside the brackets:
///
/// ```ignore
/// (GET) (/{id: u32}/foo) => {
///     ...
/// },
/// ```
///
/// Some other things to note:
///
/// - The right of the `=>` must be a block (must be surrounded by `{` and `}`).
/// - The pattern of the URL and the closure must be inside parentheses. This is to bypass
///   limitations of Rust's macros system.
/// - The default handler (with `_`) must be present or will get a compilation error.
///
// FIXME: turn `: $pt:ident` into `ty`
// TODO: don't panic if parsing fails
#[macro_export]
macro_rules! router {
    ($request:expr, $(($method:ident) ($($pat:tt)+) => $value:block,)* _ => $def:expr) => {
        {
            let request = &$request;

            // ignoring the GET parameters (everything after `?`)
            let request_url = request.url();
            let request_url = {
                let pos = request_url.find('?').unwrap_or(request_url.len());
                &request_url[..pos]
            };

            let mut ret = None;

            $({
                if ret.is_none() && request.method() == stringify!($method) {
                    ret = router!(__check_pattern request_url $value $($pat)+);
                }
            })+

            if let Some(ret) = ret {
                ret
            } else {
                $def
            }
        }
    };

    (__check_pattern $url:ident $value:block /{$p:ident} $($rest:tt)*) => (
        if !$url.starts_with('/') {
            None
        } else {
            let url = &$url[1..];
            let pat_end = url.find('/').unwrap_or(url.len());
            let rest_url = &url[pat_end..];

            if let Ok($p) = url[0 .. pat_end].parse() {
                router!(__check_pattern rest_url $value $($rest)*)
            } else {
                None
            }
        }
    );

    (__check_pattern $url:ident $value:block /{$p:ident: $t:ty} $($rest:tt)*) => (
        if !$url.starts_with('/') {
            None
        } else {
            let url = &$url[1..];
            let pat_end = url.find('/').unwrap_or(url.len());
            let rest_url = &url[pat_end..];

            if let Ok($p) = url[0 .. pat_end].parse() {
                let $p: $t = $p;
                router!(__check_pattern rest_url $value $($rest)*)
            } else {
                None
            }
        }
    );

    (__check_pattern $url:ident $value:block /$p:ident $($rest:tt)*) => (
        {
            let required = concat!("/", stringify!($p));
            if $url.starts_with(required) {
                let rest_url = &$url[required.len()..];
                router!(__check_pattern rest_url $value $($rest)*)
            } else {
                None
            }
        }
    );

    (__check_pattern $url:ident $value:block - $($rest:tt)*) => (
        {
            if $url.starts_with('-') {
                let rest_url = &$url[1..];
                router!(__check_pattern rest_url $value $($rest)*)
            } else {
                None
            }
        }
    );

    (__check_pattern $url:ident $value:block) => (
        if $url.len() == 0 { Some($value) } else { None }
    );

    (__check_pattern $url:ident $value:block /) => (
        if $url == "/" { Some($value) } else { None }
    );

    (__check_pattern $url:ident $value:block $p:ident $($rest:tt)*) => (
        {
            let required = stringify!($p);
            if $url.starts_with(required) {
                let rest_url = &$url[required.len()..];
                router!(__check_pattern rest_url $value $($rest)*)
            } else {
                None
            }
        }
    );
}

#[cfg(test)]
mod tests {
    use Request;

    #[test]
    fn basic() {
        let request = Request::fake_http("GET", "/", vec![], vec![]);

        assert_eq!(1, router!(request,
            (GET) (/hello) => { 0 },
            (GET) (/{_val:u32}) => { 0 },
            (GET) (/) => { 1 },
            _ => 0
        ));
    }

    #[test]
    fn dash() {
        let request = Request::fake_http("GET", "/a-b", vec![], vec![]);

        assert_eq!(1, router!(request,
            (GET) (/a/b) => { 0 },
            (GET) (/a_b) => { 0 },
            (GET) (/a-b) => { 1 },
            _ => 0
        ));
    }

    #[test]
    fn params() {
        let request = Request::fake_http("GET", "/hello/5", vec![], vec![]);

        assert_eq!(1, router!(request,
            (GET) (/hello/) => { 0 },
            (GET) (/hello/{id:u32}) => { if id == 5 { 1 } else { 0 } },
            (GET) (/hello/{_id:String}) => { 0 },
            _ => 0
        ));
    }

    #[test]
    #[ignore]       // TODO: not implemented
    fn param_slash() {
        let request = Request::fake_http("GET", "/hello%2F5", vec![], vec![]);

        router!(request,
            (GET) (/{a:String}) => { assert_eq!(a, "hello/5") },
            _ => panic!()
        );
    }
}
