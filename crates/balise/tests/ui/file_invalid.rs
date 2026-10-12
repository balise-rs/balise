use balise::prelude::*;

#[derive(Input)]
struct UnknownType {
    #[input(max_size = "1MB", types("image/svg+xml"))]
    logo: File,
}

#[derive(Input)]
struct NoCount {
    #[input(max_size = "5MB", types("image/png"))]
    photos: Vec<File>,
}

#[derive(Input)]
struct NotAFile {
    #[input(max_size = "1MB", types("text/plain"))]
    title: String,
}

#[derive(Input)]
struct Undeclared {
    photo: File,
}

fn main() {}
