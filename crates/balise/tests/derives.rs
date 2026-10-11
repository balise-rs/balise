//! `Input` reads fields with their rules and reports every error; `Output` writes JSON.

use balise::__private::{FieldErrors, Fields, Input, Output};
use balise::prelude::*;

#[derive(Debug, Input)]
#[input(model = Order)]
struct CreateOrder {
    #[input(default = 1, min = 1, max = 100)]
    quantity: u32,
    note: Option<String>,
    #[input(email)]
    contact: String,
    #[input(length(min = 1, max = 3))]
    tags: Vec<String>,
    #[input(one_of("standard", "express"), default = "standard")]
    shipping: String,
    #[input(url)]
    r#ref: Option<String>,
}

/// Stands for the model until the ORM: `#[input(model = …)]` only names it for now.
struct Order;

#[derive(Debug, PartialEq, Input)]
struct UpdateOrder {
    #[input(min = 1)]
    quantity: Patch<u32>,
    note: Patch<Option<String>>,
}

#[derive(Debug, Input)]
#[input(allow_unknown_fields)]
struct Webhook {
    event: String,
}

fn from_json<T: Input>(json: &str) -> Result<T, FieldErrors> {
    let value = balise_core::json::parse(json.as_bytes()).unwrap();
    T::from_fields(Fields::from_json(value).unwrap())
}

fn errors(result: Result<impl std::fmt::Debug, FieldErrors>) -> Vec<String> {
    result
        .unwrap_err()
        .into_iter()
        .map(|e| format!("{} {} {}", e.field, e.code.code(), e.message))
        .collect()
}

#[test]
fn an_input_is_read_with_its_defaults() {
    let order: CreateOrder = from_json(r#"{"contact": "a@b.co", "tags": ["gift"]}"#).unwrap();
    assert_eq!((order.quantity, order.contact.as_str()), (1, "a@b.co"));
    assert_eq!(order.note, None);
    assert_eq!(order.shipping, "standard");
    assert_eq!(order.tags, ["gift"]);
    assert_eq!(order.r#ref, None);
}

#[test]
fn every_field_error_is_reported_at_once() {
    let result = from_json::<CreateOrder>(
        r#"{"quantity": 0, "note": 3, "tags": [], "shipping": "drone",
            "ref": "ftp://x", "quantiy": 2}"#,
    );
    assert_eq!(
        errors(result),
        [
            "quantity VAL0006 must be at least 1",
            "note VAL0005 must be text, not `3`",
            "contact VAL0004 is required",
            "tags VAL0006 must have from 1 to 3 items",
            "shipping VAL0006 must be one of `standard`, `express`",
            "ref VAL0006 must be an http or https URL",
            "quantiy VAL0007 is not a field of this input; did you mean `quantity`?",
        ]
    );
}

#[test]
fn a_patch_tells_absent_from_null() {
    let update: UpdateOrder = from_json(r#"{"note": null}"#).unwrap();
    assert_eq!(
        update,
        UpdateOrder {
            quantity: Patch::Unchanged,
            note: Patch::Set(None),
        }
    );
    assert_eq!(
        errors(from_json::<UpdateOrder>(r#"{"quantity": 0}"#)),
        ["quantity VAL0006 must be at least 1"]
    );
    assert_eq!(
        errors(from_json::<UpdateOrder>(r#"{"quantity": null}"#)),
        ["quantity VAL0005 must be a value (this field cannot be cleared), not null"]
    );
}

#[test]
fn unknown_fields_pass_when_the_input_allows_them() {
    let webhook: Webhook = from_json(r#"{"event": "paid", "livemode": true}"#).unwrap();
    assert_eq!(webhook.event, "paid");
}

#[test]
fn a_query_string_is_read_from_text() {
    let pairs = [("quantity", "3"), ("contact", "a@b.co"), ("tags", "a,b")]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .to_vec();
    let order = CreateOrder::from_fields(Fields::from_query(pairs)).unwrap();
    assert_eq!((order.quantity, order.tags.len()), (3, 2));
}

#[derive(Output)]
struct LineView {
    sku: String,
    quantity: u32,
}

#[derive(Output)]
#[output(from = Invoice)]
struct InvoiceView {
    id: String,
    lines: Vec<LineView>,
    note: Option<String>,
    r#type: String,
}

struct Invoice {
    id: String,
    lines: Vec<LineView>,
    note: Option<String>,
    r#type: String,
}

#[test]
fn an_output_is_written_as_json_in_declaration_order() {
    let invoice = Invoice {
        id: "inv_1".into(),
        lines: vec![LineView {
            sku: "tea".into(),
            quantity: 2,
        }],
        note: None,
        r#type: "final".into(),
    };
    let view: InvoiceView = invoice.into();
    let mut json = String::new();
    view.write_json(&mut json);
    assert_eq!(
        json,
        r#"{"id":"inv_1","lines":[{"sku":"tea","quantity":2}],"note":null,"type":"final"}"#
    );
}
