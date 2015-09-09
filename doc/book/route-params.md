# Route parameters

The `router!` macro has another property: it can accept parameters.

To do so, simply put brackets `{` `}` around the parameter name in the URL.

```rust
router!(request,
    (GET) (/{id}) => {
        Response::text(format!("you passed {}", id))
    },

    _ => Response::text("other url")
)

// (note that this snippet doesn't compile, see below)
```

The value of the parameter will automatically be available in the following block, as if we
had written `let id = ...;`.

If you go to `http://localhost/18` you should get `you passed 18` as a response.

## Parsing error

If you try compiling the snippet above, you should get an error saying that the Rust compiler
couldn't infer the type of `id`. This is a pretty common problem.

To solve this, the `router!` macro allows you to specify the type of the variable like this:

```rust
router!(request,
    (GET) (/{id: i32}) => {
        Response::text(format!("you passed {}", id))
    },

    _ => Response::text("other url")
)
```

If the value fails to parse, then the route is simply ignored. This means that if you go to
`http://localhost/hello` you will get `other url` and not `you passed hello`.
