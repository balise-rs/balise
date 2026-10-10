//! `EXT`: errors from outside Balise that it could not classify.

codes! {
    EXT0001 {
        title: "Unclassified error",
        message: "{cause}",
        fix: "Read the original cause. If it comes from Balise or one of its dependencies, report it so it gets its own code.",
        explanation: r#"
Balise converts the errors it recognizes into errors with their own code. This one
came from outside Balise, and Balise could not classify it. The message is the
original one, unchanged.

In dev, foreign output (a line printed by a dependency or a C library) is also
wrapped in this code, so every line of output keeps the same format.

Look for the cause in your own code first: the location points to it. Do not
investigate inside the framework's dependencies.
"#,
    }
}
