`multipart` Examples
===========================

These example files show how to use `multipart` with the various crates it integrates with.

These files carry the same licenses as [`multipart` itself](https://github.com/abonander/multipart#license), though this may be lightened to a copyright-free license in the near future.

## Server

[`tiny_http`](tiny_http.rs)
---------------------------
Author: [White-Oak]

This example shows how to use `multipart` with the [`tiny_http` crate](https://crates.io/crates/tiny_http), via `multipart`'s support for the `tiny_http::Request` type.

```
$ cargo run --features tiny_http --example tiny_http
```

[Puhrez]: https://github.com/puhrez
[White-Oak]: https://github.com/white-oak

