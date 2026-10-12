//! `FILE`: uploads, from the multipart body to storage.

codes! {
    FILE0001 {
        title: "File too large",
        message: "`{field}` is larger than its limit of {limit}",
        fix: "Send a file of at most {limit}. If larger files are expected, raise `max_size` on the field in its `Input`.",
        explanation: r#"
Each `File` field declares its `max_size`. Balise counts the bytes while the file is
written to disk and stops as soon as the limit is passed: the rest is never read, and the
part already written is deleted. The answer is `413`.

The generated TypeScript client knows the limit and rejects a file that is too large
before sending it, so this error usually comes from another client. If the field is meant
to receive larger files, raise `max_size` in the `Input`: the route's body limit and
timeout follow it.
"#,
    }
    FILE0002 {
        title: "Wrong file type",
        message: "`{field}` must be {expected}; its content is {found}",
        fix: "Send a file of one of the declared types. If this type should be accepted, add it to `types(…)` on the field in its `Input`.",
        explanation: r#"
Each `File` field declares the types it accepts. Balise checks the file's content, its
first bytes, not the name or the `Content-Type` the client sent, which anyone can set. A
text type (`text/csv`, `text/plain`, `application/json`) is checked as valid UTF-8 text.
The answer is `415`.

`File::content_type()` gives the type that matched, never the one the client claimed.
"#,
    }
    FILE0003 {
        title: "Too many files",
        message: "`{field}` takes at most {max} file(s)",
        fix: "Send at most {max} file(s) in `{field}`. If more are expected, raise `max_count` on the field in its `Input`.",
        explanation: r#"
A `Vec<File>` field declares `max_count`, and a `File` or `Option<File>` field takes one
file. Balise stops reading at the first file over the count, so that the route's body
limit, computed from the declared sizes and counts, holds. The answer is `413`.
"#,
    }
    FILE0004 {
        title: "Malformed multipart body",
        message: "The request body is not valid multipart/form-data: {reason}",
        fix: "Send the body as `multipart/form-data`, as browsers and `FormData` do; never build it by hand.",
        explanation: r#"
A route whose `Input` has `File` fields reads its body as `multipart/form-data`: a boundary
declared in the `Content-Type` header, then one part per field, each with its own headers.
The body did not follow that format: the boundary is missing, a part has no name, the body
ends before its last boundary… The answer is `400`.

Browsers, `FormData` in JavaScript and the generated client always produce valid bodies.
"#,
    }
    FILE0005 {
        title: "Cannot write file",
        message: "Cannot write {what}: {cause}",
        fix: "Check that `storage.dir` exists or can be created, that the app's user can write to it, and that the disk is not full.",
        explanation: r#"
Uploaded files are written to `<storage.dir>/.uploads` while they arrive, then moved into
`storage.dir` by `File::store`. Writing failed: the folder cannot be created, the app's user
has no permission, or the disk is full. The answer is `500`.

In a container, `storage.dir` is usually a mounted volume: without one, stored files are
lost when the container is replaced.
"#,
    }
    FILE0006 {
        title: "Invalid storage folder",
        message: "`{folder}` is not a valid storage folder",
        fix: "Pass a relative folder name, such as `\"videos\"` or `\"shops/logos\"`: lowercase letters, digits, `-`, `_`, and `/` between names.",
        explanation: r#"
`File::store(folder)` moves the file into `storage.dir/<folder>`, under a name Balise
generates. The folder is part of the application's code, never of the request: it is a
relative path made of simple names, so that it cannot leave `storage.dir`.
"#,
    }
}
