`multipart` Examples
===========================

These example files show how to use `multipart` with the various crates it integrates with.

These files carry the same licenses as [`multipart` itself](https://github.com/cybergeek94/multipart#license), though this may be lightened to a copyright-free license in the near future.

More examples are underway and volunteers to create them are still needed. See [this issue](https://github.com/cybergeek94/multipart/issues/29) for more information.

##Client

Examples for the client-side integrations of `multipart`'s API.

[`hyper_client`](hyper_client.rs)
---------------------------------
Author: [cybergeek94][cybergeek94]

This example showcases usage of `multipart` with the `hyper::client::Request` API.

```
$ cargo run --example hyper_client
```

[`hyper_reqbuilder`](hyper_reqbuilder.rs)
-----------------------------------------
Author: [cybergeek94][cybergeek94]

This example showcases usage of `multipart` with Hyper's new `Client` API,
via the lazy-writing capabilities of `multipart::client::lazy`.

```
$ cargo run --example hyper_reqbuilder
```


##Server

[`hyper_server`](hyper_server.rs)
---------------------------------
Author: [Puhrez][puhrez]

This example shows how to use `multipart` with a [`hyper::Server`] (http://hyper.rs/) to intercept multipart requests.

```
$ cargo run --example hyper_server
```

[`iron`](iron.rs)
-----------------
Author: [White-Oak][white-oak]

This example shows how to use `multipart` with the [Iron web application framework](http://ironframework.io/), via `multipart`'s support
for the `iron::Request` type.

To run:

```
$ cargo run --features iron --example iron
```

[`iron_intercept`](iron_intercept.rs)
-------------------------------------
Author: [cybergeek94][cybergeek94]

This example shows how to use `multipart`'s specialized `Intercept` middleware with Iron, which reads out all fields and
files to local storage so they can be accessed arbitrarily.

```
$ cargo run --features iron --example iron_intercept
```

[`tiny_http`](tiny_http.rs)
---------------------------
Author: [White-Oak][white-oak]

This example shows how to use `multipart` with the [`tiny_http` crate](https://crates.io/crates/tiny_http), via `multipart`'s support for the `tiny_http::Request` type.

```
$ cargo run --features tiny_http --example tiny_http
```

[puhrez]: https://github.com/puhrez
[white-oak]: https://github.com/white-oak
[cybergeek94]: https://github.com/cybergeek94

