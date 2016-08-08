`multipart` Sample Projects
===========================

These are simple but fully fledged Cargo/Rust application projects to show how to use `multipart` with the various crates it integrates with.

These projects carry the same licenses as [`multipart` itself](https://github.com/cybergeek94/multipart#license), though this may be lightened to a copyright-free license in the near future.

More sample projects are underway and volunteers to create them are still needed. See [this issue](https://github.com/cybergeek94/multipart/issues/29) for more information.

[`iron`](iron.rs)
-----
Author: [White-Oak][white-oak]

This sample project shows how to use `multipart` with the [Iron web application framework](http://ironframework.io/), via `multipart`'s support
for the `iron::Request` type.

To run:

```
$ cargo run --features "iron" --example iron
```


[`tiny_http`](tiny_http.rs)
----------
Author: [White-Oak][white-oak]

This sample project shows how to use `multipart` with the [`tiny_http` crate](https://crates.io/crates/tiny_http), via `multipart`'s support for the `tiny_http::Request` type.


```
$ cargo run --features "tiny_http" --example tiny_http
```

[`hyper_server`](hyper_server.rs)
---------------------------------
Author: [Puhrez][puhrez]

This sample project shows how to use `multipart` with a [`hyper::Server`] (http://hyper.rs/) to intercept multipart requests.

```
$ cargo run --example hyper_server
```

[`nickel`](nickel.rs)
---------------------------------
Author: [iamsebastian][iamsebastian]

How you could use this multipart crate to handle multipart uploads in [nickel.rs](https://nickel.rs).

```
$ cargo run --example nickel --features nickel
```


[iamsebastian]: https://github.com/iamsebastian
[puhrez]: https://github.com/puhrez
[white-oak]: https://github.com/white-oak
