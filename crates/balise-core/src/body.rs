//! The request body, read only when a handler asks for it.
//!
//! The engine hands each request its body as a [`BodySource`], without reading it. `Body<T>`
//! is extracted after every other parameter, so a request that a cheaper parameter rejects
//! (path, query, authentication) never has its body read. The route's limit is checked on
//! every chunk: reading stops as soon as it is passed.

use std::fmt;
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

use crate::error::Error;
use crate::error_codes::{HTTP0002, HTTP0011};
use crate::text::format_size;

/// Where the bytes of a body come from: the connection, or memory in tests.
pub trait BodySource: Send {
    /// The next chunk, `None` at the end, or why the body cannot be read.
    fn poll_chunk(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<u8>, String>>>;
}

/// A body held in memory.
struct InMemory(Option<Vec<u8>>);

impl BodySource for InMemory {
    fn poll_chunk(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<u8>, String>>> {
        Poll::Ready(self.0.take().filter(|bytes| !bytes.is_empty()).map(Ok))
    }
}

/// How far the body has been read, for the timeout to tell a slow client from a slow
/// handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Nothing read yet, and maybe never.
    Waiting,
    /// Some of it was read, not all.
    Reading,
    /// Read to the end, or abandoned.
    Done,
}

/// Shared between the request and the engine.
#[derive(Debug, Clone, Default)]
pub struct ProgressHandle(Arc<AtomicU8>);

impl ProgressHandle {
    pub fn get(&self) -> Progress {
        match self.0.load(Ordering::Relaxed) {
            0 => Progress::Waiting,
            1 => Progress::Reading,
            _ => Progress::Done,
        }
    }

    fn set(&self, progress: Progress) {
        let value = match progress {
            Progress::Waiting => 0,
            Progress::Reading => 1,
            Progress::Done => 2,
        };
        self.0.store(value, Ordering::Relaxed);
    }
}

/// The body of a request, read chunk by chunk within the route's limit.
pub struct RequestBody {
    source: Option<Pin<Box<dyn BodySource>>>,
    limit: usize,
    read: usize,
    progress: ProgressHandle,
}

impl RequestBody {
    /// A body read from `source`, at most `limit` bytes.
    pub fn new(source: impl BodySource + 'static, limit: usize) -> RequestBody {
        RequestBody {
            source: Some(Box::pin(source)),
            limit,
            read: 0,
            progress: ProgressHandle::default(),
        }
    }

    /// A body already in memory, at most `limit` bytes.
    pub fn from_bytes(bytes: Vec<u8>, limit: usize) -> RequestBody {
        RequestBody::new(InMemory(Some(bytes)), limit)
    }

    /// How far it has been read.
    pub fn progress(&self) -> ProgressHandle {
        self.progress.clone()
    }

    /// The largest body this route reads.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The next chunk, `None` at the end. Past the limit, a `413` (`HTTP0011`); a client that
    /// stops sending, a `400` (`HTTP0002`).
    pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let Some(source) = self.source.as_mut() else {
            return Ok(None);
        };
        self.progress.set(Progress::Reading);
        let next = poll_fn(|cx| source.as_mut().poll_chunk(cx)).await;
        match next {
            Some(Ok(chunk)) => {
                self.read += chunk.len();
                if self.read > self.limit {
                    self.finish();
                    let limit = format_size(self.limit);
                    return Err(Error::coded(413, HTTP0011, &[("limit", &limit)]));
                }
                Ok(Some(chunk))
            }
            Some(Err(cause)) => {
                self.finish();
                Err(Error::coded(400, HTTP0002, &[("cause", &cause)]))
            }
            None => {
                self.finish();
                Ok(None)
            }
        }
    }

    /// The whole body, within the limit.
    pub async fn bytes(&mut self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.chunk().await? {
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn finish(&mut self) {
        self.source = None;
        self.progress.set(Progress::Done);
    }
}

impl Default for RequestBody {
    /// An empty body.
    fn default() -> RequestBody {
        RequestBody::from_bytes(Vec::new(), 0)
    }
}

impl fmt::Debug for RequestBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestBody")
            .field("limit", &self.limit)
            .field("read", &self.read)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a future that never waits, which is all these tests need.
    pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(std::task::Waker::noop());
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
        }
    }

    #[test]
    fn a_body_within_its_limit_is_read() {
        let mut body = RequestBody::from_bytes(b"hello".to_vec(), 5);
        assert_eq!(block_on(body.bytes()).unwrap(), b"hello");
        assert_eq!(body.progress().get(), Progress::Done);
    }

    #[test]
    fn a_body_over_its_limit_stops_with_a_413() {
        let mut body = RequestBody::from_bytes(vec![0; 2048], 1024);
        let error = block_on(body.bytes()).unwrap_err();
        assert_eq!(error.status(), 413);
        assert_eq!(
            error.message(),
            "The request body is larger than this route's limit of 1 KB"
        );
    }
}
