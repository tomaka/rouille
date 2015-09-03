

// FIXME: turn `: $pt:ident` into `ty`
// TODO: don't use a hashmap for perfs
// TODO: don't panic if parsing fails
#[macro_export]
macro_rules! router {
    ($request:expr, $(GET ($($pat:tt)+) => ($($closure:tt)+),)* _ => $def:expr) => {
        {
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
