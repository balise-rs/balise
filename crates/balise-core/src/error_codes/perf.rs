//! `PERF`: performance problems found while the app runs in dev.

codes! {
    PERF0002 {
        title: "Event loop blocked",
        message: "{task} held the event loop for {duration} without yielding",
        fix: "Move the CPU-heavy or blocking work into `balise::blocking(|| …).await`, so the server keeps answering other requests.",
        explanation: r#"
Requests share a few threads that must never wait: while a handler computes or calls a
blocking function (a synchronous file read, `std::thread::sleep`, a heavy loop), the
requests on the same thread wait too. In dev, Balise times each step of a handler or a task
and warns past 10 ms.

`balise::blocking(|| …).await` runs the work on a pool made for it. Blocking calls also have
async versions: use them when Balise or its modules provide one.
"#,
    }
}
