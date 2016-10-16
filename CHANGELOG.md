# Changelog

## Version 0.2.0

- Fixed the `+` character in the query string not being replaced with a space as it should.
- `Request::data()` now returns an `Option<impl Read>` instead of a `Vec<u8>`. If `data()` is
  called twice, the second call will return `None`.
- `RouteError` has been removed. You are now encouraged to return a `Response` everywhere instead
  of a `Result<Response, RouteError>`.
- The `try_or_400!`, `find_route!` and `assert_or_4OO!` macros and the `match_assets` function have
  been adjusted for the previous change.
- Added a `try_or_404!` macro similar to `try_or_400!`.
- In the case of a panic, the response with status code 500 that the server answers now contains a
  small text in its body, indicating the user that an internal server error occured.
- Added `Response::empty_400()`, `Response::empty_404()`, `Response::success()` and
  `Response::error()`.
