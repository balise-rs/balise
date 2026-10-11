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
    CMP0006 {
        title: "Not a handler parameter",
        message: "`{type}` cannot be a handler parameter",
        fix: "Use `Path<T>`, `Query<T>`, `Body<T>`, `RequestId`, or a service such as `Db` or `CurrentUser`.",
        explanation: r#"
Every handler parameter is provided by Balise, which must know how to build it from the
request. The parameters it knows are listed in the parameters table of the documentation:
`Path<T>` for path parameters, `Query<T>` and `Body<T>` for an `Input`, and services.

A plain type such as `u32` or `CreateOrder` is wrapped: `Path<u32>`, `Body<CreateOrder>`.
"#,
    }
    CMP0007 {
        title: "Not a handler result",
        message: "`{type}` cannot be returned from a handler",
        fix: "Return a struct that derives `Output`, or a wrapper: `Created<T>`, `NoContent`, `Page<T>`.",
        explanation: r#"
A handler's return type decides the response: an `Output` is a `200` with JSON, `Created<T>`
a `201`, `NoContent` a `204`. A model is never returned: convert it to its view with `.into()`.
"#,
    }
    CMP0008 {
        title: "Invalid Input or Output declaration",
        message: "Invalid `{derive}` declaration: {reason}",
        fix: "Derive on a struct with named fields and no generics, and use only the attributes listed by `balise explain CMP0008`.",
        explanation: r#"
`Input` and `Output` derive on structs with named fields and no generics. Their attributes
are the ones listed in the documentation, and nothing else:

- on an `Input`: `#[input(model = Order)]` and `#[input(allow_unknown_fields)]`;
- on an `Input` field: `min`, `max`, `length(min = …, max = …)`, `email`, `url`,
  `one_of("a", "b")` and `default = …`;
- on an `Output`: `#[output(from = Order)]`.

The message names what is wrong and the fix gives the right form.
"#,
    }
    CMP0009 {
        title: "Validation rule on the wrong type",
        message: "`{rule}` does not apply to `{type}`",
        fix: "Use `min` and `max` on numbers, `length` on text and lists, and `email`, `url` and `one_of` on text.",
        explanation: r#"
Each validation rule of an `Input` applies to some types only: `min` and `max` to numbers,
`length` to `String` and `Vec` (characters or items), `email`, `url` and `one_of` to
`String`. On an `Option<T>` or `Patch<T>` field, the rule applies to `T` when there is a
value. For a fixed set of values, an enum is usually clearer than `one_of`.
"#,
    }
    CMP0010 {
        title: "Field type not supported",
        message: "`{type}` cannot be a field of an `{derive}`",
        fix: "Use text, numbers, booleans and lists of them; `Option<T>` and `Patch<T>` in an `Input`; another `Output` in an `Output`.",
        explanation: r#"
Every field of an `Input` is read from JSON or a query string, and every field of an
`Output` is written as JSON, so Balise must know how. It knows `String`, `bool`, integers,
`f64` and `Vec` of them; in an `Input`, `Option<T>` for an optional field and `Patch<T>` for
a partial update; in an `Output`, `Option<T>` (written `null`) and other structs that derive
`Output`. Identifiers, enums and dates join the list with the models.
"#,
    }
}
