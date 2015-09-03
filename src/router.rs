

// FIXME: turn `: $pt:ident` into `ty`
// TODO: don't use a hashmap for perfs
// TODO: don't panic if parsing fails
#[macro_export]
macro_rules! router {
    ($request:expr, $(GET ($($pat:tt)+) => ($($closure:tt)+)),*) => {
        {
            let request = $request;
            let request_url = request.url();

            let mut ret = None;

            $({
                let mut values = std::collections::HashMap::<&'static str, &str>::new();
                if ret.is_none() && router!(__check_pattern values request_url $($pat)+) {
                    ret = router!(__parse_closure values $($closure)+);
                }
            })+

            ret
        }
    };

    (__parse_closure $values:ident |$($pn:ident $(: $pt:ident)*),*| $(-> $ret:ident)* $val:expr) => (
        {
            let closure = |$($pn$(:$pt)*),*| $(-> $ret)* { $val };
            let ret;
            loop {
                ret = Some(closure(
                    $(
                        match $values.get(stringify!($pn)).unwrap().parse() {
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
                $values.insert(stringify!($p), &url[0 .. pat_end]);
                true
            } else {
                false
            }
        }
    );

    (__check_pattern $values:ident $url:ident /$p:ident $($rest:tt)*) => (
        {
            let required = concat!("/", stringify!($p));
            let rest_url = &$url[required.len()..];
            $url.starts_with(required) && router!(__check_pattern $values rest_url $($rest)*)
        }
    );

    (__check_pattern $values:ident $url:ident) => (
        $url.len() == 0
    );

    (__check_pattern $values:ident $url:ident /) => (
        $url == "/"
    );
}
