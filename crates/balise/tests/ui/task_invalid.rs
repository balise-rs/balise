use balise::prelude::*;

#[derive(Input)]
struct Note {
    text: String,
}

fn main() {
    balise::spawn(async move |note: Body<Note>| {
        let _ = note;
        Ok(())
    });
    balise::spawn(async move || {});
}
