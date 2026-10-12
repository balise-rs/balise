//! `multipart/form-data`, read as it arrives: text fields in memory, files straight to disk.
//!
//! Each file is checked while it is written: its size against `max_size`, its first bytes
//! against the declared types, and text types as UTF-8 to the end. The first problem stops
//! the reading, and what was written is deleted.

use std::path::PathBuf;

use std::io::Write;

use crate::body::RequestBody;
use crate::error::Error;
use crate::error_codes::{FILE0001, FILE0002, FILE0003, FILE0004, FILE0005, HTTP0011};
use crate::file::{self, File, FileSpec, SNIFF_LENGTH};
use crate::input::{Fields, Raw};
use crate::text::format_size;

/// The longest boundary RFC 2046 allows.
const MAX_BOUNDARY: usize = 70;

/// The most bytes of headers one part can have.
const MAX_PART_HEADERS: usize = 16 * 1024;

/// Bytes gathered before each write to disk.
const WRITE_BUFFER: usize = 256 * 1024;

/// Reads a multipart body into fields. `text_limit` bounds the text fields together.
pub(crate) async fn read(
    body: &mut RequestBody,
    content_type: &str,
    specs: &'static [FileSpec],
    text_limit: usize,
) -> Result<Fields, Error> {
    let boundary = boundary(content_type)
        .ok_or_else(|| malformed("the `Content-Type` header has no valid `boundary`"))?;
    let mut parser = Parser::new(body, &boundary);
    let mut entries: Vec<(String, Raw)> = Vec::new();
    let mut text_size = 0;

    parser.skip_preamble().await?;
    while let Some(headers) = parser.next_part().await? {
        let part = Part::parse(&headers)?;
        let Some(filename) = part.filename else {
            let mut text = Vec::new();
            while let Some(piece) = parser.content().await? {
                text_size += piece.len();
                if text_size > text_limit {
                    let limit = format_size(text_limit);
                    return Err(Error::coded(413, HTTP0011, &[("limit", &limit)]));
                }
                text.extend_from_slice(&piece);
            }
            let text = String::from_utf8(text)
                .map_err(|_| malformed(&format!("the field `{}` is not UTF-8 text", part.name)))?;
            if !entries.iter().any(|(name, _)| *name == part.name) {
                entries.push((part.name, Raw::Text(text)));
            }
            continue;
        };

        let spec = specs.iter().find(|spec| spec.field == part.name);
        let received = entries.iter_mut().find(|(name, _)| *name == part.name);
        let count = match &received {
            Some((_, Raw::Files(files))) => files.len(),
            _ => 0,
        };
        let file = match spec {
            Some(spec) => {
                if count as u32 >= spec.max_count {
                    let (field, max) = (spec.field, spec.max_count.to_string());
                    return Err(Error::coded(
                        413,
                        FILE0003,
                        &[("field", field), ("max", &max)],
                    ));
                }
                receive(&mut parser, spec, filename, part.content_type).await?
            }
            // A file for a field that is not a file: never written, reported by the `Input`.
            None => {
                while parser.content().await?.is_some() {}
                None
            }
        };
        match (received, file) {
            (Some((_, Raw::Files(files))), Some(file)) => files.push(file),
            (Some(_), _) => {}
            (None, file) => {
                if file.is_some() || spec.is_none() {
                    entries.push((part.name, Raw::Files(file.into_iter().collect())));
                }
            }
        }
    }
    Ok(Fields::from_entries(entries))
}

/// Writes one file to disk while checking it. `None` for an empty file input, which
/// browsers send with an empty name and no content.
async fn receive(
    parser: &mut Parser<'_>,
    spec: &'static FileSpec,
    filename: String,
    claimed: Option<String>,
) -> Result<Option<File>, Error> {
    let uploads = file::uploads();
    let path = uploads.join(format!("u_{}", crate::text::unique_id()));
    let guard = Guard(Some(path.clone()));
    let write_error = |cause: std::io::Error| {
        let what = format!("the upload of `{}`", spec.field);
        Error::coded(
            500,
            FILE0005,
            &[("what", &what), ("cause", &cause.to_string())],
        )
        .with_cause(cause)
    };
    let created = path.clone();
    let output = file::off_loop(move || {
        std::fs::create_dir_all(uploads)?;
        std::fs::File::create(created)
    })
    .await
    .map_err(write_error)?;
    let mut output = Writer {
        file: Some(output),
        buffer: Vec::with_capacity(WRITE_BUFFER),
    };

    let mut size: u64 = 0;
    let mut head: Vec<u8> = Vec::new();
    // Every piece is checked as UTF-8 until one is not: that decides a text type, and costs
    // nothing more on binary files, which fail within their first bytes.
    let mut utf8 = Utf8::default();
    let mut check = None;
    while let Some(piece) = parser.content().await? {
        size += piece.len() as u64;
        if size > spec.max_size {
            let limit = format_size(usize::try_from(spec.max_size).unwrap_or(usize::MAX));
            let args = [("field", spec.field), ("limit", limit.as_str())];
            return Err(Error::coded(413, FILE0001, &args));
        }
        utf8.feed(&piece);
        if check.is_none() {
            let wanted = SNIFF_LENGTH.saturating_sub(head.len()).min(piece.len());
            head.extend_from_slice(&piece[..wanted]);
            if head.len() >= SNIFF_LENGTH {
                check = Some(Check::decide(spec, &head, claimed.as_deref())?);
            }
        }
        if let Some(Check::Text(_)) = check
            && !utf8.valid
        {
            return Err(not_text(spec, &head));
        }
        output.write(&piece).await.map_err(write_error)?;
    }
    if size == 0 && filename.is_empty() {
        return Ok(None);
    }
    let check = match check {
        Some(check) => check,
        None => Check::decide(spec, &head, claimed.as_deref())?,
    };
    let content_type = match check {
        Check::Binary(mime) => mime,
        Check::Text(mime) if utf8.valid && utf8.pending.is_empty() => mime,
        Check::Text(_) => return Err(not_text(spec, &head)),
    };
    output.flush().await.map_err(write_error)?;
    guard.disarm();
    Ok(Some(File::uploaded(path, filename, content_type, size)))
}

/// Writes a file in large blocks, each on the blocking pool.
struct Writer {
    file: Option<std::fs::File>,
    buffer: Vec<u8>,
}

impl Writer {
    async fn write(&mut self, piece: &[u8]) -> std::io::Result<()> {
        self.buffer.extend_from_slice(piece);
        match self.buffer.len() >= WRITE_BUFFER {
            true => self.flush().await,
            false => Ok(()),
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        let (Some(mut output), buffer) = (self.file.take(), std::mem::take(&mut self.buffer))
        else {
            return Ok(());
        };
        let (output, mut buffer) = file::off_loop(move || {
            output.write_all(&buffer)?;
            Ok((output, buffer))
        })
        .await?;
        buffer.clear();
        (self.file, self.buffer) = (Some(output), buffer);
        Ok(())
    }
}

/// Deletes a partial upload when reading stops early.
struct Guard(Option<PathBuf>);

impl Guard {
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// How a file's type is checked, once its first bytes are known.
enum Check {
    /// Its first bytes matched this declared type.
    Binary(&'static str),
    /// A declared text type: the whole content must be UTF-8.
    Text(&'static str),
}

impl Check {
    fn decide(spec: &FileSpec, head: &[u8], claimed: Option<&str>) -> Result<Check, Error> {
        if let Some(mime) = file::binary_match(spec.types, head) {
            return Ok(Check::Binary(mime));
        }
        let texts: Vec<&'static str> = spec
            .types
            .iter()
            .copied()
            .filter(|mime| file::is_text(mime))
            .collect();
        // The type the client claimed, when it is one of the declared text types.
        let claimed = claimed.and_then(|claimed| texts.iter().find(|t| **t == claimed));
        match claimed.or(texts.first()) {
            Some(mime) => Ok(Check::Text(mime)),
            None => Err(wrong_type(spec, &file::describe(head))),
        }
    }
}

/// UTF-8 checked piece by piece: a character may be cut between two pieces.
struct Utf8 {
    valid: bool,
    /// The start of a character cut at the end of the last piece.
    pending: Vec<u8>,
}

impl Default for Utf8 {
    fn default() -> Utf8 {
        Utf8 {
            valid: true,
            pending: Vec::new(),
        }
    }
}

impl Utf8 {
    fn feed(&mut self, piece: &[u8]) {
        if !self.valid {
            return;
        }
        let checked = match self.pending.is_empty() {
            true => std::str::from_utf8(piece)
                .map(|_| ())
                .map_err(|e| (e, piece)),
            false => {
                // Only the cut character needs the bytes before it.
                let mut joined = std::mem::take(&mut self.pending);
                joined.extend_from_slice(piece);
                return self.feed_owned(joined);
            }
        };
        match checked {
            Ok(()) => {}
            Err((error, bytes)) if error.error_len().is_none() => {
                self.pending = bytes[error.valid_up_to()..].to_vec();
            }
            Err(_) => self.valid = false,
        }
    }

    fn feed_owned(&mut self, bytes: Vec<u8>) {
        match std::str::from_utf8(&bytes) {
            Ok(_) => {}
            Err(error) if error.error_len().is_none() => {
                self.pending = bytes[error.valid_up_to()..].to_vec();
            }
            Err(_) => self.valid = false,
        }
    }
}

fn not_text(spec: &FileSpec, head: &[u8]) -> Error {
    let found = match file::describe(head).as_str() {
        "text" => "text that is not valid UTF-8".to_string(),
        other => other.to_string(),
    };
    wrong_type(spec, &found)
}

fn wrong_type(spec: &FileSpec, found: &str) -> Error {
    let expected = file::list(spec.types);
    Error::coded(
        415,
        FILE0002,
        &[
            ("field", spec.field),
            ("expected", &expected),
            ("found", found),
        ],
    )
}

fn malformed(reason: &str) -> Error {
    Error::coded(400, FILE0004, &[("reason", reason)])
}

/// The boundary of `multipart/form-data; boundary=…`.
fn boundary(content_type: &str) -> Option<String> {
    let (_, params) = content_type.split_once(';')?;
    let boundary = parameters(params)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("boundary"))?
        .1;
    (!boundary.is_empty() && boundary.len() <= MAX_BOUNDARY).then_some(boundary)
}

/// `name="photo"; filename="a.png"` as pairs. Values may be quoted, with `\"` inside.
fn parameters(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| *c == ';' || c.is_whitespace()) {
            chars.next();
        }
        let name: String =
            std::iter::from_fn(|| chars.next_if(|c| *c != '=' && *c != ';')).collect();
        if name.is_empty() {
            return pairs;
        }
        let mut value = String::new();
        if chars.next_if_eq(&'=').is_some() {
            if chars.next_if_eq(&'"').is_some() {
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => value.extend(chars.next()),
                        c => value.push(c),
                    }
                }
            } else {
                value = std::iter::from_fn(|| chars.next_if(|c| *c != ';')).collect();
            }
        }
        pairs.push((
            name.trim().to_ascii_lowercase(),
            value.trim_end().to_string(),
        ));
    }
}

/// The headers of one part that matter.
struct Part {
    name: String,
    /// Present, even empty, when the part is a file.
    filename: Option<String>,
    /// The type the client claimed, used only to choose between declared text types.
    content_type: Option<String>,
}

impl Part {
    fn parse(headers: &str) -> Result<Part, Error> {
        let mut disposition = None;
        let mut content_type = None;
        for line in headers.split("\r\n").filter(|line| !line.is_empty()) {
            let Some((name, value)) = line.split_once(':') else {
                return Err(malformed("a part has a header without `:`"));
            };
            match name.trim().to_ascii_lowercase().as_str() {
                "content-disposition" => disposition = Some(value.trim().to_string()),
                "content-type" => {
                    let essence = value.split(';').next().unwrap_or_default();
                    content_type = Some(essence.trim().to_ascii_lowercase());
                }
                _ => {}
            }
        }
        let disposition =
            disposition.ok_or_else(|| malformed("a part has no `Content-Disposition` header"))?;
        let (kind, params) = disposition.split_once(';').unwrap_or((&disposition, ""));
        if !kind.trim().eq_ignore_ascii_case("form-data") {
            return Err(malformed("a part is not `form-data`"));
        }
        let params = parameters(params);
        let get = |key: &str| {
            params
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        let name = get("name")
            .filter(|name| !name.is_empty())
            .ok_or_else(|| malformed("a part has no field name"))?;
        // `filename*=UTF-8''…` carries a name that is not ASCII, and wins.
        let encoded = get("filename*").and_then(|value| {
            let (charset, rest) = value.split_once('\'')?;
            let (_, encoded) = rest.split_once('\'')?;
            charset
                .eq_ignore_ascii_case("utf-8")
                .then(|| crate::text::percent_decode(encoded))
        });
        Ok(Part {
            name,
            filename: encoded.or_else(|| get("filename")),
            content_type,
        })
    }
}

/// Finds parts in the body, keeping in memory only what has not been handed out yet.
struct Parser<'a> {
    body: &'a mut RequestBody,
    buffer: Vec<u8>,
    /// `\r\n--boundary`: what ends each part's content.
    delimiter: Vec<u8>,
    /// Whether the current part's content is finished.
    part_done: bool,
}

impl<'a> Parser<'a> {
    fn new(body: &'a mut RequestBody, boundary: &str) -> Parser<'a> {
        Parser {
            body,
            // The first boundary has no line break before it: one is added, so that every
            // boundary looks the same.
            buffer: b"\r\n".to_vec(),
            delimiter: format!("\r\n--{boundary}").into_bytes(),
            part_done: true,
        }
    }

    /// Adds the next chunk; `false` at the end of the body.
    async fn fill(&mut self) -> Result<bool, Error> {
        match self.body.chunk().await? {
            Some(chunk) => {
                self.buffer.extend_from_slice(&chunk);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Discards what comes before the first boundary.
    async fn skip_preamble(&mut self) -> Result<(), Error> {
        loop {
            if let Some(at) = find(&self.buffer, &self.delimiter) {
                self.buffer.drain(..at + self.delimiter.len());
                return Ok(());
            }
            let keep = self.delimiter.len() - 1;
            if self.buffer.len() > keep {
                self.buffer.drain(..self.buffer.len() - keep);
            }
            if !self.fill().await? {
                return Err(malformed("the body has no boundary"));
            }
        }
    }

    /// After a boundary: the next part's headers, or `None` after the last one.
    async fn next_part(&mut self) -> Result<Option<String>, Error> {
        // Skip what is left of the previous part.
        while self.content().await?.is_some() {}
        loop {
            // Whitespace may follow a boundary.
            let blank = self
                .buffer
                .iter()
                .take_while(|b| **b == b' ' || **b == b'\t')
                .count();
            self.buffer.drain(..blank);
            if self.buffer.len() >= 2 {
                break;
            }
            if !self.fill().await? {
                return Err(malformed("the body ends after a boundary"));
            }
        }
        if self.buffer.starts_with(b"--") {
            return Ok(None);
        }
        if !self.buffer.starts_with(b"\r\n") {
            return Err(malformed("a boundary is not followed by a line break"));
        }
        // The line break after the boundary starts the search, so that a part without
        // headers is `\r\n\r\n`.
        loop {
            if let Some(at) = find(&self.buffer, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&self.buffer[2.min(at)..at]).into_owned();
                self.buffer.drain(..at + 4);
                self.part_done = false;
                return Ok(Some(headers));
            }
            if self.buffer.len() > MAX_PART_HEADERS {
                return Err(malformed("the headers of a part are too long"));
            }
            if !self.fill().await? {
                return Err(malformed("the body ends inside the headers of a part"));
            }
        }
    }

    /// The next piece of the current part's content, `None` at its end.
    async fn content(&mut self) -> Result<Option<Vec<u8>>, Error> {
        if self.part_done {
            return Ok(None);
        }
        loop {
            if let Some(at) = find(&self.buffer, &self.delimiter) {
                let piece: Vec<u8> = self.buffer.drain(..at).collect();
                self.buffer.drain(..self.delimiter.len());
                self.part_done = true;
                return Ok(Some(piece));
            }
            // The end of the buffer may be the start of a boundary: it is kept.
            let keep = self.delimiter.len() - 1;
            if self.buffer.len() > keep {
                let piece = self.buffer.drain(..self.buffer.len() - keep).collect();
                return Ok(Some(piece));
            }
            if !self.fill().await? {
                return Err(malformed("the body ends inside a part"));
            }
        }
    }
}

/// The first position of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let first = *needle.first()?;
    let mut start = 0;
    while start + needle.len() <= haystack.len() {
        let last = haystack.len() - needle.len();
        let offset = haystack[start..=last].iter().position(|b| *b == first)?;
        let at = start + offset;
        if haystack[at..at + needle.len()] == *needle {
            return Some(at);
        }
        start = at + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::BodySource;
    use crate::input::{FileField, InputField};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// A body that arrives in chunks of `size` bytes, to cut boundaries anywhere.
    struct Chunked(Vec<u8>, usize);

    impl BodySource for Chunked {
        fn poll_chunk(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Vec<u8>, String>>> {
            let size = self.1.min(self.0.len());
            Poll::Ready((size > 0).then(|| Ok(self.0.drain(..size).collect())))
        }
    }

    fn run<F: std::future::Future>(future: F) -> F::Output {
        let storage = std::env::temp_dir().join("balise-core-tests");
        file::set_storage(&storage.display().to_string());
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0";

    static PHOTO: [FileSpec; 2] = [
        FileSpec {
            field: "photo",
            max_size: 1024,
            types: &["image/png", "image/jpeg"],
            max_count: 1,
        },
        FileSpec {
            field: "rows",
            max_size: 1024,
            types: &["text/csv"],
            max_count: 2,
        },
    ];

    fn form(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
        let mut body = b"preamble\r\n".to_vec();
        for (name, filename, content) in parts {
            body.extend_from_slice(b"--XyZ\r\n");
            let disposition = match filename {
                Some(file) => format!("form-data; name=\"{name}\"; filename=\"{file}\""),
                None => format!("form-data; name=\"{name}\""),
            };
            body.extend_from_slice(
                format!("Content-Disposition: {disposition}\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(content);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--XyZ--\r\n");
        body
    }

    fn read_form(body: Vec<u8>, chunk: usize) -> Result<Fields, Error> {
        run(async {
            let mut body = RequestBody::new(Chunked(body, chunk), usize::MAX);
            read(&mut body, "multipart/form-data; boundary=XyZ", &PHOTO, 1024).await
        })
    }

    #[test]
    fn fields_and_files_are_read_whatever_the_chunk_size() {
        let mut png = PNG.to_vec();
        // A boundary-like sequence inside the content is not a boundary.
        png.extend_from_slice(b"\r\n--XyY and more");
        let body = form(&[
            ("title", None, b"Holidays"),
            ("photo", Some("beach.png"), &png),
            (
                "rows",
                Some("a.csv"),
                "nom,prix\n\u{e9}t\u{e9},3\n".as_bytes(),
            ),
            ("rows", Some("b.csv"), b"x\n"),
        ]);
        for chunk in [1, 3, 7, 64, 4096] {
            let mut fields = read_form(body.clone(), chunk).unwrap();
            assert_eq!(fields.read::<String>("title").as_deref(), Some("Holidays"));
            let photo: File = fields.read_file("photo").unwrap();
            assert_eq!(
                (photo.name(), photo.content_type(), photo.size()),
                ("beach.png", "image/png", png.len() as u64)
            );
            assert_eq!(run(photo.read()).unwrap(), png);
            let rows: Vec<File> = fields.read_file("rows").unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].content_type(), "text/csv");
            assert!(fields.finish(&["title", "photo", "rows"], false).is_empty());
        }
    }

    #[test]
    fn a_file_is_deleted_with_its_value() {
        let mut fields = read_form(form(&[("photo", Some("a.png"), PNG)]), 16).unwrap();
        let photo: File = fields.read_file("photo").unwrap();
        let path = photo.path_for_tests().to_path_buf();
        assert!(path.exists());
        drop(photo);
        assert!(!path.exists());
    }

    fn code(result: Result<Fields, Error>) -> (u16, String) {
        let error = result.err().expect("an error");
        (
            error.status(),
            format!("{} {}", error.code(), error.message()),
        )
    }

    #[test]
    fn each_problem_stops_the_reading_with_its_code() {
        let big = [PNG, &[0; 2000]].concat();
        assert_eq!(
            code(read_form(form(&[("photo", Some("a.png"), &big)]), 100)),
            (
                413,
                "FILE0001 `photo` is larger than its limit of 1 KB".into()
            )
        );
        assert_eq!(
            code(read_form(
                form(&[("photo", Some("a.png"), b"%PDF-1.7 ...")]),
                100
            )),
            (
                415,
                "FILE0002 `photo` must be image/png or image/jpeg; its content is application/pdf"
                    .into()
            )
        );
        assert_eq!(
            code(read_form(
                form(&[("rows", Some("a.csv"), b"\0\x80\xfe\x01")]),
                100
            )),
            (
                415,
                "FILE0002 `rows` must be text/csv; its content is not a recognized type".into()
            )
        );
        let three = form(&[
            ("rows", Some("a.csv"), b"x"),
            ("rows", Some("b.csv"), b"x"),
            ("rows", Some("c.csv"), b"x"),
        ]);
        assert_eq!(
            code(read_form(three, 100)),
            (413, "FILE0003 `rows` takes at most 2 file(s)".into())
        );
        let full = form(&[("title", None, b"some text")]);
        let cut = full[..full.len() - 14].to_vec();
        assert_eq!(
            code(read_form(cut, 100)),
            (
                400,
                "FILE0004 The request body is not valid multipart/form-data: the body ends inside a part"
                    .into()
            )
        );
        let no_boundary = run(async {
            let mut body = RequestBody::from_bytes(Vec::new(), 10);
            read(&mut body, "multipart/form-data", &PHOTO, 10).await
        });
        assert_eq!(code(no_boundary).0, 400);
    }

    #[test]
    fn an_empty_file_input_is_absent() {
        let mut fields = read_form(form(&[("photo", Some(""), b"")]), 100).unwrap();
        let photo: Option<File> = fields.read_file("photo").unwrap();
        assert!(photo.is_none());
    }

    #[test]
    fn a_file_where_text_is_expected_is_a_field_error() {
        let mut fields = read_form(form(&[("title", Some("a.png"), PNG)]), 100).unwrap();
        assert_eq!(fields.read::<String>("title"), None);
        let errors = fields.finish(&["title"], false);
        assert_eq!(errors[0].message, "must be text, not a file");
        let _ = <String as InputField>::read;
        let _ = <File as FileField>::read;
    }

    #[test]
    fn parameters_are_read_quoted_or_not() {
        assert_eq!(
            boundary("multipart/form-data; boundary=\"a b\""),
            Some("a b".into())
        );
        assert_eq!(
            boundary("multipart/form-data; charset=x; boundary=abc"),
            Some("abc".into())
        );
        let part = Part::parse(
            "Content-Disposition: form-data; name=\"doc\"; filename=\"a\\\"b.txt\"; filename*=UTF-8''%C3%A9t%C3%A9.txt",
        )
        .ok()
        .unwrap();
        assert_eq!(
            (part.name.as_str(), part.filename.as_deref()),
            ("doc", Some("été.txt"))
        );
    }
}
