
/// Equivalent to a `match` expression but for routes.
///
/// Here is an example usage:
///
/// ```ignored      // FIXME: 
/// # #[macro_use] extern crate rouille; fn main() {
/// # let request: rouille::Request = unsafe { std::mem::uninitialized() };
/// router!(request,
///     // first route
///     GET (/) => (|| {
///         12
///     }),
///
///     // other routes here
///
///     _ => || 5
/// )
/// # }
/// ```
///
/// The macro will take each route one by one and execute the first one that matches, similar to a
/// `match` language construct. The whole `router!` expression then returns what the closure
/// returns, therefore they must all return the same type.
///
/// You can use parameters by putting them inside `{}`:
///
/// ```ignore
/// GET (/{id}/foo) => (|id: u32| {
///     ...
/// }),
/// ```
///
/// If you use parameters inside `{}`, then a field with the same name must exist in the closure's
/// parameters list. The parameters do not need be in the same order.
///
/// Each parameter gets parsed through the `FromStr` trait. If the parsing fails, the route is
/// ignored.
///
/// Some other things to note:
///
/// - The right of the `=>` must have a closure-like syntax.
/// - The pattern of the URL and the closure must be inside parentheses. This is to bypass
///   limitations of Rust's macros system.
/// - The default handler (with `_`) must be present or will get a compilation error.
///
// FIXME: turn `: $pt:ident` into `ty`
// TODO: don't use a hashmap for perfs
// TODO: don't panic if parsing fails
#[macro_export]
macro_rules! router {
    ($request:expr, $(GET ($($pat:tt)+) => ($($closure:tt)+),)* _ => $def:expr) => {
        {
            use std;

            let ref request = $request;

            // ignoring the GET parameters (everything after `?`)
            let request_url = {
                let url = request.url();
                let pos = url.find('?').unwrap_or(url.len());
                &url[..pos]
            };

            let mut ret = None;

            $({
                // we use a RefCell just to avoid warnings about `values doesn't need to be mutable`
                let values = std::cell::RefCell::new(std::collections::HashMap::<&'static str, &str>::new());
                if ret.is_none() && router!(__check_pattern values request_url $($pat)+) {
                    let values = values.borrow();
                    ret = router!(__parse_closure values $($closure)+);
                }
            })+

            if let Some(ret) = ret {
                ret
            } else {
                $def()
            }
        }
    };

    (__parse_closure $values:ident |$($pn:ident $(: $pt:ident)*),*| $(-> $ret:ident)* $val:expr) => (
        {
            let closure = |$($pn$(:$pt)*),*| $(-> $ret)* { $val };
            let ret;
            loop {
                ret = Some(closure(
                    $(
                        match $values.get(stringify!($pn))
                                     .expect("Closure parameter missing from route").parse()
                        {
                            Ok(val) => val,
                            Err(_) => { ret = None; break; }
                        }
                    ),*
                ));
                break;
            }
            ret
        }
    );

    (__parse_closure $values:ident || $val:expr) => (
        {
            let closure = || $val;
            assert!($values.len() == 0);
            Some(closure())
        }
    );

    (__check_pattern $values:ident $url:ident /{$p:ident} $($rest:tt)*) => (
        if !$url.starts_with('/') {
            false
        } else {
            let url = &$url[1..];
            let pat_end = url.find('/').unwrap_or(url.len());
            let rest_url = &url[pat_end..];
            if router!(__check_pattern $values rest_url $($rest)*) {
                $values.borrow_mut().insert(stringify!($p), &url[0 .. pat_end]);
                true
            } else {
                false
            }
        }
    );

    (__check_pattern $values:ident $url:ident /$p:ident $($rest:tt)*) => (
        {
            let required = concat!("/", stringify!($p));
            if $url.starts_with(required) {
                let rest_url = &$url[required.len()..];
                router!(__check_pattern $values rest_url $($rest)*)
            } else {
                false
            }
        }
    );

    (__check_pattern $values:ident $url:ident) => (
        $url.len() == 0
    );

    (__check_pattern $values:ident $url:ident /) => (
        $url == "/"
    );
}
