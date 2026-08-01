use proc_macro2::Span;
use quote::quote;

use crate::convert::value_to_expr;

fn v2e(value: &serde_value::Value) -> Result<syn::Expr, syn::Error> {
    value_to_expr(value, None, Span::call_site())
}

#[test]
#[allow(clippy::approx_constant)]
fn value_to_expr_unit() {
    let expr = v2e(&serde_value::Value::Unit).unwrap();
    assert!(matches!(expr, syn::Expr::Tuple(tuple) if tuple.elems.is_empty()));
}

#[test]
fn value_to_expr_bool_true() {
    let expr = v2e(&serde_value::Value::Bool(true)).unwrap();
    assert!(
        matches!(expr, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) if b.value())
    );
}

#[test]
fn value_to_expr_bool_false() {
    let expr = v2e(&serde_value::Value::Bool(false)).unwrap();
    assert!(
        matches!(expr, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) if !b.value())
    );
}

#[test]
fn value_to_expr_i32() {
    let expr = v2e(&serde_value::Value::I32(42)).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("42"), "should contain 42, got {s}");
}

#[test]
fn value_to_expr_u64() {
    let expr = v2e(&serde_value::Value::U64(1000)).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("1000"), "should contain 1000, got {s}");
}

#[test]
fn value_to_expr_f64_precision() {
    let expr = v2e(&serde_value::Value::F64(3.141_592_65)).unwrap();
    let s = quote! { #expr }.to_string();
    // Should preserve precision, not truncate to "3.1"
    assert!(s.contains("3.14"), "should preserve precision, got {s}");
}

#[test]
fn value_to_expr_f32_precision() {
    let expr = v2e(&serde_value::Value::F32(2.718)).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("2.718"), "should preserve precision, got {s}");
}

#[test]
fn value_to_expr_string() {
    let expr = v2e(&serde_value::Value::String("hello".to_string())).unwrap();
    assert!(matches!(
        expr,
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(_),
            ..
        })
    ));
}

#[test]
fn value_to_expr_char() {
    let expr = v2e(&serde_value::Value::Char('X')).unwrap();
    assert!(matches!(
        expr,
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Char(_),
            ..
        })
    ));
}

#[test]
fn value_to_expr_option_none() {
    let expr = v2e(&serde_value::Value::Option(None)).unwrap();
    assert!(matches!(expr, syn::Expr::Path(p) if p.path.segments.last().unwrap().ident == "None"));
}

#[test]
fn value_to_expr_option_some() {
    let inner = Box::new(serde_value::Value::String("inner".to_string()));
    let expr = v2e(&serde_value::Value::Option(Some(inner))).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("Some"), "should contain Some");
    assert!(s.contains("inner"), "should contain inner value");
}

#[test]
fn value_to_expr_seq() {
    let items = vec![
        serde_value::Value::I32(1),
        serde_value::Value::I32(2),
        serde_value::Value::I32(3),
    ];
    let expr = v2e(&serde_value::Value::Seq(items)).unwrap();
    // Seq produces a macro invocation (vec![...]).
    assert!(
        matches!(expr, syn::Expr::Macro(_)),
        "should produce a macro invocation"
    );
}

#[test]
fn value_to_expr_empty_map() {
    let entries: std::collections::BTreeMap<serde_value::Value, serde_value::Value> =
        std::collections::BTreeMap::new();
    let expr = v2e(&serde_value::Value::Map(entries)).unwrap();
    // Empty map produces an ExprStruct with placeholder path.
    assert!(
        matches!(expr, syn::Expr::Struct(_)),
        "should produce a struct expression"
    );
}

#[test]
fn value_to_expr_map_with_fields() {
    use std::collections::BTreeMap;
    let mut entries = BTreeMap::new();
    entries.insert(
        serde_value::Value::String("name".to_string()),
        serde_value::Value::String("test".to_string()),
    );
    entries.insert(
        serde_value::Value::String("count".to_string()),
        serde_value::Value::I32(5),
    );
    let expr = v2e(&serde_value::Value::Map(entries)).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("name"), "should contain name field, got {s}");
    assert!(s.contains("count"), "should contain count field, got {s}");
}

#[test]
fn value_to_expr_bytes() {
    let bytes = vec![72, 101, 108, 108, 111];
    let expr = v2e(&serde_value::Value::Bytes(bytes)).unwrap();
    // Bytes produces a macro invocation (vec![...]).
    assert!(
        matches!(expr, syn::Expr::Macro(_)),
        "should produce a macro invocation"
    );
}

#[test]
fn value_to_expr_nested_option() {
    let inner = Box::new(serde_value::Value::Option(Some(Box::new(
        serde_value::Value::I32(42),
    ))));
    let expr = v2e(&serde_value::Value::Option(Some(inner))).unwrap();
    let s = quote! { #expr }.to_string();
    assert!(s.contains("Some"), "should contain Some");
    assert!(s.contains("42"), "should contain nested value");
}
