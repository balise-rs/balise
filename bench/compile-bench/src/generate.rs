//! `compile-bench generate`: the reference project, built from the mock-up.
//!
//! The mock-up's `orders` resource is the template: it is copied once per resource and renamed,
//! so the reference project follows the mock-up whenever it changes. The rest of the mock-up
//! (`users`, `auth`, seeds, configuration) is copied as is. The front end is left out: only
//! Rust is measured.

use std::fs;
use std::path::{Path, PathBuf};

use crate::measure::{BODY_MARKER, FIELD_MARKER, MODEL_MARKER, collect_rust_files};

pub const USAGE: &str = "compile-bench generate <out> [--resources N] [--mockup DIR]";

/// Default number of resources: with `auth`, about 50 routes.
const DEFAULT_RESOURCES: usize = 8;

/// The template resource, as named in the mock-up.
const TEMPLATE: Names = Names::new("order", "orders", "Order", "Orders", "ORD");

/// Resources of the reference project, the template first.
const RESOURCES: &[Names] = &[
    TEMPLATE,
    Names::new("invoice", "invoices", "Invoice", "Invoices", "INV"),
    Names::new("ticket", "tickets", "Ticket", "Tickets", "TIC"),
    Names::new("project", "projects", "Project", "Projects", "PRJ"),
    Names::new("task", "tasks", "Task", "Tasks", "TSK"),
    Names::new("document", "documents", "Document", "Documents", "DOC"),
    Names::new("payment", "payments", "Payment", "Payments", "PAY"),
    Names::new("shipment", "shipments", "Shipment", "Shipments", "SHP"),
    Names::new("booking", "bookings", "Booking", "Bookings", "BKG"),
    Names::new("review", "reviews", "Review", "Reviews", "REV"),
    Names::new("article", "articles", "Article", "Articles", "ART"),
    Names::new("contract", "contracts", "Contract", "Contracts", "CTR"),
];

/// Identifiers that contain the template's name but belong to Balise's API.
const KEEP: &[&str] = &["order_by"];

#[derive(Clone, Copy)]
struct Names {
    singular: &'static str,
    plural: &'static str,
    pascal: &'static str,
    pascal_plural: &'static str,
    code_prefix: &'static str,
}

impl Names {
    const fn new(
        singular: &'static str,
        plural: &'static str,
        pascal: &'static str,
        pascal_plural: &'static str,
        code_prefix: &'static str,
    ) -> Names {
        Names {
            singular,
            plural,
            pascal,
            pascal_plural,
            code_prefix,
        }
    }
}

pub fn main(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut out = None;
    let mut resources = DEFAULT_RESOURCES;
    let mut mockup = PathBuf::from("mockup");
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--resources" => {
                resources = value()?.parse().map_err(|_| "--resources needs a number")?
            }
            "--mockup" => mockup = value()?.into(),
            _ if out.is_none() => out = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    let out = out.ok_or(format!("usage: {USAGE}"))?;
    generate(&mockup, &out, resources)?;
    println!(
        "reference project with {resources} resources in {}",
        out.display()
    );
    Ok(())
}

fn generate(mockup: &Path, out: &Path, resources: usize) -> Result<(), String> {
    if resources == 0 || resources > RESOURCES.len() {
        return Err(format!(
            "--resources must be between 1 and {}",
            RESOURCES.len()
        ));
    }
    let resources = &RESOURCES[..resources];
    if out.exists() {
        fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }

    write(&out.join("Cargo.toml"), &manifest(mockup)?)?;
    copy(&mockup.join("balise.toml"), &out.join("balise.toml"))?;
    write(&out.join("src/main.rs"), &main_rs(mockup, resources)?)?;
    for dir in ["users", "auth", "seeds"] {
        copy_dir(
            &mockup.join("src").join(dir),
            &out.join("src").join(dir),
            |text| text.to_string(),
        )?;
    }

    let template = mockup.join("src").join(TEMPLATE.plural);
    for names in resources {
        let dir = out.join("src").join(names.plural);
        copy_dir(&template, &dir, |text| rename(text, names))?;
    }
    insert_markers(&out.join("src").join(TEMPLATE.plural))
}

/// The mock-up's manifest, with Balise taken from this repository.
fn manifest(mockup: &Path) -> Result<String, String> {
    let facade = fs::canonicalize(mockup.join("../crates/balise"))
        .map_err(|e| format!("Balise's facade next to {}: {e}", mockup.display()))?;
    let text = read(&mockup.join("Cargo.toml"))?;
    let mut found = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| match line.strip_prefix("balise = { version = ") {
            Some(rest) => {
                found = true;
                let features = rest.split_once(", ").map_or("}", |(_, features)| features);
                format!(
                    "balise = {{ path = {:?}, {features}",
                    facade.display().to_string()
                )
            }
            None => line.replace("name = \"shop\"", "name = \"reference\""),
        })
        .collect();
    if !found {
        return Err("no `balise = { version = … }` dependency in the mock-up's Cargo.toml".into());
    }
    Ok(lines.join("\n") + "\n\n[workspace]\n")
}

/// The mock-up's `main.rs`, with one `mod` line per resource instead of the template's.
fn main_rs(mockup: &Path, resources: &[Names]) -> Result<String, String> {
    let text = read(&mockup.join("src/main.rs"))?;
    let template_line = format!("mod {};", TEMPLATE.plural);
    if !text.lines().any(|line| line == template_line) {
        return Err(format!("no `{template_line}` in the mock-up's main.rs"));
    }
    let mods: Vec<String> = resources
        .iter()
        .map(|names| format!("mod {};", names.plural))
        .collect();
    Ok(text.replace(&template_line, &mods.join("\n")))
}

/// Renames every identifier and word derived from the template's name.
fn rename(text: &str, names: &Names) -> String {
    let mut result = String::with_capacity(text.len());
    let mut word = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            result.push_str(&rename_word(&word, names));
            word.clear();
            result.push(c);
        }
    }
    result.push_str(&rename_word(&word, names));
    result
}

fn rename_word(word: &str, names: &Names) -> String {
    if word.is_empty() || KEEP.contains(&word) {
        return word.to_string();
    }
    // Error codes: `ORD0001` becomes `INV0001`.
    if let Some(digits) = word.strip_prefix(TEMPLATE.code_prefix)
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return format!("{}{digits}", names.code_prefix);
    }
    // snake_case: `orders`, `list_shows_only_own_orders`.
    if word.contains('_') || word.chars().all(|c| !c.is_ascii_uppercase()) {
        let segments: Vec<&str> = word
            .split('_')
            .map(|segment| match segment {
                s if s == TEMPLATE.singular => names.singular,
                s if s == TEMPLATE.plural => names.plural,
                s => s,
            })
            .collect();
        return segments.join("_");
    }
    // PascalCase: `Order`, `CreateOrder`, `OrderView`.
    humps(word)
        .into_iter()
        .map(|hump| match hump {
            h if h == TEMPLATE.pascal => names.pascal,
            h if h == TEMPLATE.pascal_plural => names.pascal_plural,
            h => h,
        })
        .collect()
}

/// Splits `CreateOrder` into `Create` and `Order`.
fn humps(word: &str) -> Vec<&str> {
    let mut humps = Vec::new();
    let mut start = 0;
    for (i, c) in word.char_indices().skip(1) {
        if c.is_ascii_uppercase() {
            humps.push(&word[start..i]);
            start = i;
        }
    }
    humps.push(&word[start..]);
    humps
}

/// Marks where `measure` edits: a handler body, an input and the model.
fn insert_markers(resource: &Path) -> Result<(), String> {
    let markers = [
        ("mod.rs", "async fn create(", BODY_MARKER),
        (
            "api.rs",
            &format!("pub struct Create{} {{", TEMPLATE.pascal) as &str,
            FIELD_MARKER,
        ),
        (
            "model.rs",
            &format!("pub struct {} {{", TEMPLATE.pascal),
            MODEL_MARKER,
        ),
    ];
    for (file, anchor, marker) in markers {
        let path = resource.join(file);
        let text = read(&path)?;
        let Some(line) = text
            .lines()
            .find(|line| line.trim_start().starts_with(anchor))
        else {
            return Err(format!(
                "no line starting with `{anchor}` in {}",
                path.display()
            ));
        };
        let indent = line.len() - line.trim_start().len() + 4;
        let marked = text.replacen(line, &format!("{line}\n{}{marker}", " ".repeat(indent)), 1);
        write(&path, &marked)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path, transform: impl Fn(&str) -> String) -> Result<(), String> {
    let mut files = Vec::new();
    collect_rust_files(from, &mut files)?;
    for file in files {
        let relative = file.strip_prefix(from).map_err(|e| e.to_string())?;
        write(&to.join(relative), &transform(&read(&file)?))?;
    }
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    write(to, &read(from)?)
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mockup() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mockup")
    }

    fn generated(name: &str) -> PathBuf {
        let out = std::env::temp_dir().join("compile-bench-tests").join(name);
        generate(&mockup(), &out, DEFAULT_RESOURCES).unwrap();
        out
    }

    fn sources(project: &Path) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        collect_rust_files(&project.join("src"), &mut files).unwrap();
        files.sort();
        files
            .into_iter()
            .map(|file| {
                let text = fs::read_to_string(&file).unwrap();
                (file.strip_prefix(project).unwrap().to_path_buf(), text)
            })
            .collect()
    }

    #[test]
    fn renames_every_form_of_the_name() {
        let names = &RESOURCES[1];
        let text = "Order OrderId CreateOrder orders order list_own_orders ORD0001 order_by";
        assert_eq!(
            rename(text, names),
            "Invoice InvoiceId CreateInvoice invoices invoice list_own_invoices INV0001 order_by"
        );
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(sources(&generated("first")), sources(&generated("second")));
    }

    #[test]
    fn has_about_fifty_routes() {
        let project = generated("counts");
        let all: String = sources(&project)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        let routes = ["#[get(", "#[post(", "#[put(", "#[patch(", "#[delete("]
            .iter()
            .map(|attribute| all.matches(attribute).count())
            .sum::<usize>();
        let models = all.matches("#[derive(Model)]").count();
        assert!((45..=60).contains(&routes), "{routes} routes");
        assert_eq!(
            models,
            DEFAULT_RESOURCES + 1,
            "one model per resource, plus User"
        );
    }

    #[test]
    fn copies_keep_nothing_of_the_template() {
        let project = generated("residue");
        for (path, text) in sources(&project) {
            // The template's own copy, and the files that use it, keep its name.
            if path.starts_with(Path::new("src").join(TEMPLATE.plural))
                || path.starts_with("src/seeds")
                || path == Path::new("src/main.rs")
            {
                continue;
            }
            let renamed = rename(&text, &RESOURCES[1]);
            assert_eq!(
                text,
                renamed,
                "{} still mentions the template",
                path.display()
            );
        }
    }

    #[test]
    fn markers_are_in_the_template_copy_only() {
        let project = generated("markers");
        for marker in [BODY_MARKER, FIELD_MARKER, MODEL_MARKER] {
            let count = sources(&project)
                .iter()
                .filter(|(_, text)| text.lines().any(|line| line.trim() == marker))
                .count();
            assert_eq!(count, 1, "`{marker}` must appear once");
        }
    }
}
