
/// Evaluates each parameter until one of them evaluates to something else
/// than `Err(RouteError::NoRouteFound)`.
///
/// This macro supposes that each route returns a `Result<_, RouteError>`.
///
/// # Example
///
/// ```no_run
/// ```
///
#[macro_export]
macro_rules! find_route {
    ($($handler:expr),+) => ({
        let mut response = Err($crate::RouteError::NoRouteFound);
        $(
            if let Err($crate::RouteError::NoRouteFound) = response {
                response = $handler;
            }
        )+
        response
    });
}
