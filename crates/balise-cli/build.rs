//! Embeds the project template (`template/`) in the binary, so that `balise new` needs no
//! file next to it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("template");
    println!("cargo::rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    let mut out = String::from("/// Every file of the template: its path, then its content.\n");
    out.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (relative, absolute) in files {
        let _ = writeln!(out, "    ({relative:?}, include_bytes!({absolute:?})),");
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("template.rs");
    std::fs::write(dest, out).unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            println!("cargo::rerun-if-changed={}", path.display());
            collect(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            let relative = relative.to_string_lossy().replace('\\', "/");
            files.push((relative, path.display().to_string()));
        }
    }
}
