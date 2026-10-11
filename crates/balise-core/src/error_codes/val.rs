//! `VAL`: input validation.

codes! {
    VAL0001 {
        title: "Invalid input",
        message: "Invalid fields: {names}",
        fix: "Fix each field listed in `fields`: each one has its own code and message.",
        explanation: r#"
The request reached a handler, but its body or query string does not satisfy the `Input`
it declares. The answer is `422` and the handler is not called. `fields` holds every field
that did not pass, each with its own code, so that all of them can be fixed at once:

- `VAL0004`: a required field is missing;
- `VAL0005`: a value has the wrong type;
- `VAL0006`: a value breaks a rule (`min`, `max`, `length`, `email`…);
- `VAL0007`: a field is not declared by the `Input`.

The same rules run in the browser, generated from the same `Input`, so a form usually shows
these messages before sending anything.
"#,
    }
    VAL0002 {
        title: "Invalid JSON body",
        message: "The request body is not a JSON object: {cause}",
        fix: "Send a JSON object, such as `{\"quantity\": 2}`, with `Content-Type: application/json`.",
        explanation: r#"
The route reads a JSON body (`Body<T>`), which must be one JSON object whose keys are the
fields of the `Input`. Either the bytes received are not valid JSON, and the cause gives the
line and column where reading stopped, or they are valid JSON of another kind (a list, a
string…). The answer is `400`.
"#,
    }
    VAL0003 {
        title: "Unsupported content type",
        message: "The request body must be JSON, not `{content_type}`",
        fix: "Send the body as JSON with the header `Content-Type: application/json`.",
        explanation: r#"
The route reads a JSON body (`Body<T>`), and the request declares another content type.
The answer is `415`. The generated client always sends JSON.
"#,
    }
    VAL0004 {
        title: "Missing field",
        message: "is required",
        fix: "Send the field. If it is optional by nature, declare it as `Option<T>` or give it a `default` in the `Input`.",
        explanation: r#"
A field of the `Input` is absent from the body or the query string, or is `null`, and the
`Input` neither marks it optional (`Option<T>`) nor gives it a default. It appears in
`fields` of a `VAL0001` error.
"#,
    }
    VAL0005 {
        title: "Wrong type",
        message: "must be {expected}, not {found}",
        fix: "Send a value of the type the message names, such as a number without quotes for an integer.",
        explanation: r#"
A field of the `Input` is present, but its value cannot be read as the declared type: text
for an integer, an object for a string, a number too large for its type. In a query string
every value is text, and numbers and booleans are read from it. It appears in `fields` of a
`VAL0001` error.
"#,
    }
    VAL0006 {
        title: "Rule not met",
        message: "{rule}",
        fix: "Send a value that satisfies the rule the message states, or change the rule on the `Input` if it is wrong.",
        explanation: r#"
A field has the right type, but breaks a rule declared on the `Input`: `min`, `max`,
`length`, `email`, `url` or `one_of`. The message states the rule and its bound
(`must be at most 100`). It appears in `fields` of a `VAL0001` error.
"#,
    }
    VAL0007 {
        title: "Unknown field",
        message: "is not a field of this input{suggestion}",
        fix: "Remove the field, or fix its name: the message suggests the closest declared one. If the input comes from a third party that adds fields, mark it `#[input(allow_unknown_fields)]`.",
        explanation: r#"
The body or the query string has a field that the `Input` does not declare. It is rejected
rather than ignored, so that a typo (`quantiy`) never passes silently while the real field
takes its default. The message suggests the closest declared name when there is one. It
appears in `fields` of a `VAL0001` error.

An `Input` that describes a payload sent by someone else, such as a webhook, accepts
unknown fields with `#[input(allow_unknown_fields)]`: they are ignored.
"#,
    }
}
