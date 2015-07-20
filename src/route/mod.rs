use std::any::Any;
use std::ops::Deref;
use std::path::PathBuf;

use input::Input;
use output::Output;
use service::ServiceAccess;
use service::StaticServices;

use hyper::server::request::Request as HyperRequest;
use hyper::server::response::Response as HyperResponse;
use hyper::uri::RequestUri as HyperRequestUri;
use hyper::method::Method as HyperMethod;

pub struct Route {
    pub url: Pattern,
    pub method: MethodsMask,
    pub handler: Box<Handler + Send + Sync>,
}

impl Route {
    /// Returns the route parameters, or `Err` if this route can't handle the given request.
    pub fn matches(&self, request: &HyperRequest) -> Result<Box<Any>, ()> {
        if !self.method.matches(&request.method) {
            return Err(());
        }

        match request.uri {
            HyperRequestUri::AbsolutePath(ref p) => self.url.parse(p),
            _ => Err(())
        }
    }
}

/// Represents a URL pattern.
///
/// Contains a function that will return the route parameters, or `Err` if it doesn't match the
/// request's URL.
pub struct Pattern(pub Box<Fn(&str) -> Result<Box<Any>, ()> + Send + Sync>);

impl Pattern {
    /// Return `Err` if this pattern doesn't match the given URL, otherwise returns the
    /// route parameters.
    pub fn parse(&self, url: &str) -> Result<Box<Any>, ()> {
        (self.0)(url)
    }
}

/// Describes which methods must be used by the request for a route to be used.
pub struct MethodsMask {
    /// True if the `GET` method matches this mask.
    pub get: bool,
    /// True if the `POST` method matches this mask.
    pub post: bool,
    /// True if the `PUT` method matches this mask.
    pub put: bool,
    /// True if the `DELETE` method matches this mask.
    pub delete: bool,
}

impl MethodsMask {
    /// Parses from a string of the `route!` macro.
    pub fn parse(_: &str) -> MethodsMask {
        // FIXME:
        MethodsMask {
            get: true,
            post: false,
            put: false,
            delete: false,
        }
    }

    /// Returns true if the mask contains the specified method.
    pub fn matches(&self, method: &HyperMethod) -> bool {
        match method {
            &HyperMethod::Get => self.get,
            &HyperMethod::Post => self.post,
            &HyperMethod::Put => self.put,
            &HyperMethod::Delete => self.delete,
            _ => false
        }
    }
}

pub struct Router {
    /// List of the routes to try to match. They will be tried in this order.
    pub routes: Vec<Route>,
}

/// Describes types that can process a route.
pub trait Handler {
    /// Handles a request.
    fn call(&self, HyperRequest, HyperResponse, &StaticServices, route_params: &Box<Any>);
}

impl<I, O> Handler for fn(I) -> O where I: Input, O: Output {
    fn call(&self, request: HyperRequest, response: HyperResponse, services: &StaticServices,
            _: &Box<Any>)
    {
        let input = match I::process(request) {
            Ok(i) => i,
            Err(_) => return        // TODO: handle properly
        };

        let output = (*self)(input);
        output.send(response, services);
    }
}

impl<'a, I, O, S1> Handler for fn(I, S1) -> O
                               where I: Input, O: Output, S1: ServiceAccess<'a>
{
    fn call(&self, request: HyperRequest, response: HyperResponse, services: &StaticServices,
            route_params: &Box<Any>)
    {
        let input = match I::process(request) {
            Ok(i) => i,
            Err(_) => return        // TODO: handle properly
        };

        // TODO: Properly handling lifetimes here would require HKTs which are not supported
        //       by Rust. Considering that services are never destroyed, it's ok to cast their
        //       lifetime to whatever we want ; however there is a danger with route parameters.
        let s1 = S1::load(unsafe { ::std::mem::transmute(services) },
                          unsafe { ::std::mem::transmute(route_params) });

        let output = (*self)(input, s1);
        output.send(response, services);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params<'a, T: 'a>(&'a T);

impl<'a, T: 'a> ServiceAccess<'a> for Params<'a, T> where T: Any {
    fn load(_: &'a StaticServices, params: &'a Box<Any>) -> Params<'a, T> {
        Params(params.downcast_ref().unwrap())      // TODO: don't panic
    }
}

impl<'a, T> Deref for Params<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

// TODO: fix the trailing '/' problem

#[macro_export]
macro_rules! router {
    (__parse_route $routes:ident [$($ex:tt)*] / $($t:tt)+) => (
        {
            let method = router!(__parse_method $($ex)*);
            router!(__parse_route2 $routes method [/] $($t)+)
        }
    );

    (__parse_route $routes:ident [$($ex:tt)*] $f:tt $($t:tt)*) => (
        router!(__parse_route $routes [$($ex)* $f] $($t)*)
    );

    (__parse_route $routes:ident []) => ();

    (__parse_route2 $routes:ident $method:ident [$($ex:tt)*] => $handler:expr, $($t:tt)*) => (
        {
            let pattern = router!(__parse_pattern $($ex)*);
            let handler = Box::new($handler) as Box<$crate::route::Handler + Send + Sync>;
            $routes.push($crate::route::Route {
                url: pattern,
                method: $method,
                handler: handler,
            });

            router!(__parse_route $routes [] $($t)*);
        }
    );

    (__parse_route2 $routes:ident $method:ident [$($ex:tt)*] => $handler:expr) => (
        router!(__parse_route2 $routes $method [$($ex)*] => $handler,)
    );

    (__parse_route2 $routes:ident $method:ident [$($ex:tt)*] $f:tt $($t:tt)*) => (
        router!(__parse_route2 $routes $method [$($ex)* $f] $($t)*)
    );

    (__parse_pattern $($t:tt)*) => (
        $crate::route::Pattern(Box::new(move |input| {
            router!(__parse_pattern_inner (input.trim_right_matches('/')) () $($t)*)
        }))
    );

    (__parse_pattern_inner ($input:expr) () [ $s:ident ]) => (
        {
            if $input.len() != 0 {
                return Err(());
            }

            Ok(Box::new($s))
        }
    );

    (__parse_pattern_inner ($input:expr) ($(, $mem:ident:$val:expr)+) [ $s:ident ]) => (
        {
            if $input.len() != 0 {
                return Err(());
            }

            Ok(Box::new($s {
                $(
                    $mem: match $val.parse() {
                        Ok(r) => r,
                        Err(_) => return Err(())
                    },
                )*
            }))
        }
    );

    (__parse_pattern_inner ($input:expr) ()) => (
        {
            if $input.len() != 0 {
                return Err(());
            }

            Ok(Box::new(()))
        }
    );

    (__parse_pattern_inner ($input:expr) ($($t:tt)*) / [ $s:ident ]) => (
        router!(__parse_pattern_inner ($input) ($($t)*) [$s])
    );

    (__parse_pattern_inner ($input:expr) ($($t:tt)*) /) => (
        router!(__parse_pattern_inner ($($t)*) ($input))
    );

    (__parse_pattern_inner ($input:expr) ($($e:tt)*) / { $val:ident } $($t:tt)*) => (
        {
            if !$input.starts_with('/') {
                return Err(());
            }

            let end = $input[1 ..].find('/').map(|p| p + 1).unwrap_or($input.len());
            let matched = &$input[1 .. end];

            router!(__parse_pattern_inner (&$input[end ..]) ($($e)* , $val: matched) $($t)*)
        }
    );

    (__parse_pattern_inner ($input:expr) ($($e:tt)*) / $val:ident $($t:tt)*) => (
        {
            let s = concat!("/", stringify!($val));
            if !$input.starts_with(s) {
                return Err(());
            }

            router!(__parse_pattern_inner (&$input[s.len() ..]) ($($e)*) $($t)*)
        }
    );

    (__parse_method $($t:tt)*) => (
        // TODO: 
        $crate::route::MethodsMask { get: true, post: false, put: false, delete: false }
    );

    // main route
    ($($t:tt)+) => (
        {
            let mut routes = Vec::new();
            router!(__parse_route routes [] $($t)+);
            $crate::route::Router { routes: routes }
        }
    );
}
