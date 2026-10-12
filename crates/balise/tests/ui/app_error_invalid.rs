use balise::prelude::*;

#[derive(AppError)]
enum ServerSide {
    #[error(status = 503, code = "ORD0001", message = "Down", fix = "Wait.")]
    Down,
}

#[derive(AppError)]
enum Reserved {
    #[error(status = 409, code = "HTTP0001", message = "Taken", fix = "Pick another.")]
    Taken,
}

#[derive(AppError)]
enum NoMessage {
    #[error(status = 409, code = "ORD0002", fix = "Cancel it first.")]
    NotCancelled,
}

#[derive(AppError)]
enum Twice {
    #[error(status = 409, code = "ORD0003", message = "A", fix = "A.")]
    A,
    #[error(status = 409, code = "ORD0003", message = "B", fix = "B.")]
    B,
}

fn main() {}
