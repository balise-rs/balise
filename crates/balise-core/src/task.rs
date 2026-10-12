//! Background tasks: what `balise::spawn` runs.
//!
//! A task is an `async` closure whose parameters are services, like a handler's:
//! `async move |cache: State<PriceCache>| { … }`. Values are captured with `move`. A task
//! returns `Result<()>`, so `?` works in it; its errors are logged with where it was spawned.

use std::future::Future;
use std::pin::Pin;

use crate::error::Error;

/// A task parameter that Balise provides: a service, such as `State<T>`. A wrong
/// parameter is reported on the task as a whole (`CMP0016`).
pub trait FromTask: Sized {
    fn from_task() -> impl Future<Output = Result<Self, Error>> + Send;
}

/// The future of a task, ready to run.
pub type TaskFuture = Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;

/// What `balise::spawn` accepts: an `async` closure with service parameters, returning
/// `Result<()>`.
#[diagnostic::on_unimplemented(
    message = "CMP0016: this cannot be spawned as a task",
    label = "not a task",
    note = "fix: Pass an `async move` closure whose parameters are services (`State<T>`, and `Db` or `Mailer` with their modules) and whose body ends with `Ok(())`; capture other values with `move`.",
    note = "more: balise explain CMP0016"
)]
pub trait Task<Params> {
    fn into_future(self) -> TaskFuture;
}

macro_rules! task {
    ($($param:ident),*) => {
        impl<F, Fut, $($param),*> Task<($($param,)*)> for F
        where
            F: FnOnce($($param),*) -> Fut + Send + 'static,
            Fut: Future<Output = Result<(), Error>> + Send + 'static,
            $($param: FromTask + Send + 'static,)*
        {
            #[allow(non_snake_case)]
            fn into_future(self) -> TaskFuture {
                Box::pin(async move {
                    $(let $param = $param::from_task().await?;)*
                    self($($param),*).await
                })
            }
        }
    };
}

task!();
task!(A);
task!(A, B);
task!(A, B, C);
task!(A, B, C, D);
