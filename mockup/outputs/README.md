# Outputs

What Balise prints for the mock-up project, written by hand before any renderer exists.
They become the reference files of the renderer's tests: the real output must match them.

| Case | What happens |
| --- | --- |
| [1-server-error](1-server-error/) | `POST /api/orders` while the database is down: a `500` |
| [2-not-found](2-not-found/) | `GET /api/orders/{id}` on another user's order: a `404`, logged on the request line only |
| [3-n-plus-one](3-n-plus-one/) | an admin dashboard that loads each order's user in a loop: a `PERF` warning |
| [4-compile-error](4-compile-error/) | a query on `Order` without `.owned_by()`: `cargo check` versus `balise check` |

Files: `dev.txt` is the dev terminal, `balise-check*.txt` and `cargo-check.txt` the terminal after a check (`-hook` is what the agent receives on stderr, with exit code 2), `balise-check.json` the same check for tools and MCP, `prod.jsonl` the JSON lines of test and prod, `response-*.json` the HTTP body the client receives, `code.rs` the code that produces the case when it is not in the mock-up.

## Colors in the terminal

The `.txt` files are plain text. In a terminal:

- level: `ERROR` red, `WARN` yellow; a request line is red for `5xx`, yellow for `4xx`, dim for `2xx`;
- the code (`DB0003`) bold;
- `at …` and code excerpts cyan;
- `fix:` green;
- the `more:` line dim.

With `NO_COLOR`, or when the output is not a terminal (hooks, CI), the text is exactly the content of the files.

## Rules these files follow

- Every error and warning: code, message, location in application code, fix, how to learn more. Never a path inside Balise or a dependency.
- In the terminal, every `fix:` line is followed by `more: balise explain CODE`. JSON has the `code` field instead.
- The request ID of an event is on its `at` line, so an event can be matched to its request when requests interleave.
- A request line reads as a sentence: `POST /api/orders → request req_7f3a2c (orders::create returned 500 in 5004 ms with 0 queries)`.
- A `5xx` is an `error` event, followed by the request line. A `4xx` is not an error of the server: no separate event; the request line carries its code, and in dev the code, message, location and fix follow it.
- One request, one line, always: method, path, status, handler, duration, queries, request ID.
- Logs may say more than the response: the server knows why a row is hidden, the client must not.

## How tests use these files

Each file is checked at two levels:

- **Renderer**: the test builds the event described here with its fixed values (time, request ID, duration), renders it, and compares with the file byte for byte. The renderer is a pure function: the clock and ids come from the event.
- **End to end**: the test runs the real case on the mock-up's code (`POST /orders` with an unreachable database, a `GET` on another user's order, the N+1 loop), captures the log stream and the response, then normalizes both the capture and the file (`<time>`, `<req>`, `<ms>`) and compares them byte for byte. This is what proves the requirements on real errors: the location points to application code and not inside Balise, the driver error is classified (`DB0003`, not `EXT0001`), the cause is kept, sensitive data is redacted, a `4xx` produces no error event.
- **Compile errors**: `code.rs` is compiled in a test project, and the output of `balise check`, with paths normalized, is compared with `balise-check.txt`.
