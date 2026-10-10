//! Consistency of the error code registry.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use balise_core::error_codes::{self, AREAS};

#[test]
fn codes_are_their_area_prefix_and_four_digits() {
    for (prefix, codes) in AREAS {
        for code in *codes {
            let digits = code
                .code()
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("{} is declared in the {prefix} area", code.code()));
            assert!(
                digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()),
                "{} must be {prefix} followed by four digits",
                code.code()
            );
        }
    }
}

#[test]
fn prefixes_have_the_expected_shape() {
    for (prefix, _) in AREAS {
        assert!(
            (2..=4).contains(&prefix.len()) && prefix.bytes().all(|b| b.is_ascii_uppercase()),
            "area prefix {prefix} must be 2 to 4 uppercase letters"
        );
    }
}

#[test]
fn codes_are_unique_and_sorted() {
    let mut seen = HashSet::new();
    for code in error_codes::all() {
        assert!(
            seen.insert(code.code()),
            "{} is declared twice",
            code.code()
        );
    }
    for (prefix, codes) in AREAS {
        assert!(
            codes.windows(2).all(|w| w[0].code() < w[1].code()),
            "codes in the {prefix} area must be sorted"
        );
    }
}

#[test]
fn every_field_is_filled() {
    for code in error_codes::all() {
        for (field, value) in [
            ("title", code.title()),
            ("message", code.message()),
            ("fix", code.fix()),
            ("explanation", code.explanation()),
        ] {
            assert!(
                !value.trim().is_empty(),
                "{} has an empty {field}",
                code.code()
            );
        }
    }
}

#[test]
fn message_placeholders_are_well_formed() {
    for code in error_codes::all() {
        let mut rest = code.message();
        while let Some(start) = rest.find(['{', '}']) {
            let tail = &rest[start..];
            assert!(tail.starts_with('{'), "{}: unmatched `}}`", code.code());
            let end = tail
                .find('}')
                .unwrap_or_else(|| panic!("{}: unclosed `{{`", code.code()));
            let name = &tail[1..end];
            assert!(
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{}: placeholder `{{{name}}}` must be snake_case",
                code.code()
            );
            rest = &tail[end + 1..];
        }
    }
}

#[test]
fn every_code_in_the_sources_is_registered() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    collect_rust_files(&crates, &mut files);
    assert!(!files.is_empty());

    let mut unregistered = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file).unwrap();
        for (index, line) in source.lines().enumerate() {
            for word in line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                if looks_like_a_code(word) && error_codes::find(word).is_none() {
                    let file = file.strip_prefix(&crates).unwrap_or(&file);
                    unregistered.push(format!("{}:{}: {word}", file.display(), index + 1));
                }
            }
        }
    }
    assert!(
        unregistered.is_empty(),
        "unregistered error codes:\n{}",
        unregistered.join("\n")
    );
}

/// Two to four uppercase letters followed by four digits, such as `EXT0001`.
fn looks_like_a_code(word: &str) -> bool {
    let letters = word.bytes().take_while(u8::is_ascii_uppercase).count();
    let digits = &word[letters..];
    (2..=4).contains(&letters) && digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit())
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}
