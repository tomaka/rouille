#!/usr/bin/env run-cargo-script
//! ```cargo
//! [dependencies]
//! juniper = "*"
//! rouille = "*"
//! serde_json = "*"
//! serde = "*"
//! ```

#[macro_use] extern crate rouille;
#[macro_use] extern crate juniper;
extern crate serde_json;

use std::io::prelude::*;
use juniper::http::{GraphQLRequest};
use juniper::{FieldResult, EmptyMutation};

struct Query;

graphql_object!(Query: Ctx |&self| {
    field hello(&executor) -> FieldResult<String> {
        // Use the special &executor argument to fetch the response string
        Ok(executor.context().0.clone())
    }
});

// Arbitrary context data.
struct Ctx(String);

// A root schema consists of a query and a mutation.
// Request queries can be executed against a RootNode.
type Schema = juniper::RootNode<'static, Query, EmptyMutation<Ctx>>;

fn main() {
    eprintln!("Open http://0.0.0.0:12000");
    rouille::start_server("0.0.0.0:12000", move |request| {
        router!(request,
            (GET) (/) => {
                // Builds a `Response` object that contains the "hello world" text.
                rouille::Response::html(r#"
<script src="https://cdnjs.cloudflare.com/ajax/libs/jquery/3.3.1/jquery.min.js"></script>
<script src="https://cdn.rawgit.com/keithhackbarth/submitAsJSON/3b674774/submitAsJson.js"></script>
<h1>hello world</h1>
<form action="/graphql" method="post" onsubmit="event.preventDefault(); submitAsJSON(this);">
<input type="text" name="query" value="query { hello }">
<button type="submit">Submit</button>
</form>
"#)
            },

            (POST) (/graphql) => {
                let mut data = request.data().unwrap();
                let mut buf = Vec::new();
                match data.read_to_end(&mut buf) {
                    Ok(_) => {}
                    Err(_) => return rouille::Response::text("Failed to read body"),
                }

                // Create a context object.
                let ctx = Ctx("world!".to_string());

                // Populate the GraphQL request object.
                let req = serde_json::from_slice::<GraphQLRequest>(&mut buf).unwrap();

                // Run the executor.
                let res = req.execute(
                    &Schema::new(Query, EmptyMutation::new()),
                    &ctx,
                );
                rouille::Response::json(&res)
            },

            _ => rouille::Response::empty_404()
        )
    });
}
