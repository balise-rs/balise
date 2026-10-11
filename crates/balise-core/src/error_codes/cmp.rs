//! `CMP`: compile-time errors raised by Balise's macros and `balise check`.

codes! {
    CMP0001 {
        title: "Handler is not async",
        message: "`{function}` must be an `async fn` to be a handler",
        fix: "Write `async fn {function}(…)`.",
        explanation: r#"
A route attribute (`#[get]`, `#[post]`…) goes on an `async fn`: handlers wait for the
database and other services without blocking the server.
"#,
    }
    CMP0002 {
        title: "Two request bodies",
        message: "`{function}` has {count} `Body` parameters; a request has one body",
        fix: "Keep one `Body<T>` parameter, with every field in one `Input` struct.",
        explanation: r#"
A request has a single body, so a handler takes at most one `Body<T>`. To receive several
values, put them all in the same `Input` struct.
"#,
    }
    CMP0003 {
        title: "Path parameters do not match the route",
        message: "`{function}` does not read the path parameters of its route: `{path}` has {expected}, the handler reads {found}",
        fix: "Add one `Path<T>` per `{name}` in the route, in order, or one tuple: `Path<(A, B)>`.",
        explanation: r#"
Each `{name}` in a route's path is read by a `Path` parameter, in the order they appear:
one `Path<T>` per parameter, or one `Path<(A, B)>` for all of them.

```rust
#[get("/shops/{shop_id}/orders/{id}")]
async fn get(shop_id: Path<ShopId>, id: Path<OrderId>, …)
```
"#,
    }
    CMP0004 {
        title: "Invalid route path",
        message: "Invalid route path `{path}`: {reason}",
        fix: "Write a path that starts with `/`, with segments separated by `/` and parameters written `{name}`.",
        explanation: r#"
A route path starts with `/`, never includes the API prefix (`/api` is added by Balise), and
writes each parameter as a whole segment: `/orders/{id}`, not `/orders/id-{id}`. Parameter
names are snake_case and unique in the path.
"#,
    }
    CMP0005 {
        title: "Invalid route attribute",
        message: "Invalid route attribute: {reason}",
        fix: "Write the attribute with the path as a string: `#[get(\"/orders/{id}\")]`.",
        explanation: r#"
A route attribute takes the path as a string literal, and goes on a function:
`#[get("/orders")]` above `async fn list(…)`.
"#,
    }
}
