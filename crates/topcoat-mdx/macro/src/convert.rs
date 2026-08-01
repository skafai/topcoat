use proc_macro2::Span;

// ---------------------------------------------------------------------------
// serde_value::Value -> syn::Expr conversion
// ---------------------------------------------------------------------------

/// Converts a `serde_value::Value` into a `syn::Expr` that constructs the
/// equivalent Rust value at compile time.
///
/// If `root_type` is provided and the value is a `Map`, it is used as the
/// struct path for the generated `ExprStruct`.  This is needed because the
/// caller emits the expression directly (not prefixed with a type).
pub(crate) fn value_to_expr(
    value: &serde_value::Value,
    root_type: Option<&syn::Type>,
    span: Span,
) -> Result<syn::Expr, syn::Error> {
    match value {
        serde_value::Value::Bool(b) => Ok(syn::parse_quote! { #b }),
        serde_value::Value::I8(n) => Ok(make_lit_int(&format!("{n}i8"), span)),
        serde_value::Value::I16(n) => Ok(make_lit_int(&format!("{n}i16"), span)),
        serde_value::Value::I32(n) => Ok(make_lit_int(&format!("{n}i32"), span)),
        serde_value::Value::I64(n) => Ok(make_lit_int(&format!("{n}i64"), span)),
        serde_value::Value::U8(n) => Ok(make_lit_int(&format!("{n}u8"), span)),
        serde_value::Value::U16(n) => Ok(make_lit_int(&format!("{n}u16"), span)),
        serde_value::Value::U32(n) => Ok(make_lit_int(&format!("{n}u32"), span)),
        serde_value::Value::U64(n) => Ok(make_lit_int(&format!("{n}u64"), span)),
        serde_value::Value::F32(n) => Ok(make_lit_float(&format!("{n:?}f32"), span)),
        serde_value::Value::F64(n) => Ok(make_lit_float(&format!("{n:?}f64"), span)),
        serde_value::Value::Char(c) => Ok(syn::parse_quote! { #c }),
        serde_value::Value::String(s) => Ok(syn::parse_quote! { #s }),
        serde_value::Value::Unit => Ok(syn::parse_quote! { () }),
        serde_value::Value::Option(None) => Ok(syn::parse_quote! { None }),
        serde_value::Value::Option(Some(inner)) => {
            let inner_expr = value_to_expr(inner, None, span)?;
            Ok(syn::parse_quote! { Some(#inner_expr) })
        }
        serde_value::Value::Newtype(inner) => value_to_expr(inner, None, span),
        serde_value::Value::Seq(items) => {
            let exprs: Result<Vec<syn::Expr>, syn::Error> =
                items.iter().map(|v| value_to_expr(v, None, span)).collect();
            let expr_list = exprs?;
            Ok(syn::parse_quote! { vec![#(#expr_list),*] })
        }
        serde_value::Value::Map(entries) => {
            // Convert map entries to struct-like field initializers.
            // When `root_type` is `Some`, the top-level Map is rendered as a
            // typed struct literal (e.g. `BlogMeta { title, date }`).
            // Nested Maps (recursive calls with `root_type = None`) fall back
            // to a placeholder `_ { ... }` path, which is only valid inside
            // `parse_quote!` — the expression won't compile as standalone Rust.
            // This is acceptable because frontmatter deserialization always
            // passes the root type, and nested maps are rendered as field
            // values (vecs, strings, etc.) rather than struct literals.
            let mut named_fields = Vec::new();
            for (key, val) in entries {
                let serde_value::Value::String(field_name) = key else {
                    return Err(syn::Error::new(
                        span,
                        format!("mdx_page! frontmatter map key is not a string: {key:?}"),
                    ));
                };
                let field_ident = syn::Ident::new(field_name, span);
                let field_expr = value_to_expr(val, None, span)?;
                named_fields.push(syn::FieldValue {
                    attrs: vec![],
                    member: syn::Member::Named(field_ident),
                    colon_token: Some(syn::token::Colon::default()),
                    expr: field_expr,
                });
            }
            // Use the provided root type as the struct path. If not given,
            // fall back to a placeholder so the expression still parses.
            let path = match root_type {
                Some(syn::Type::Path(tp)) => tp.path.clone(),
                _ => syn::Path::from(syn::Ident::new("_", span)),
            };
            Ok(syn::Expr::Struct(syn::ExprStruct {
                attrs: vec![],
                qself: None,
                path,
                brace_token: syn::token::Brace::default(),
                dot2_token: None,
                rest: None,
                fields: named_fields.into_iter().collect(),
            }))
        }
        serde_value::Value::Bytes(b) => {
            // Bytes in frontmatter are unusual; encode as a vec of u8 values.
            let bytes: Vec<syn::Expr> = b
                .iter()
                .map(|v| make_lit_int(&format!("{v}u8"), span))
                .collect();
            Ok(syn::parse_quote! { vec![#(#bytes),*] })
        }
    }
}

/// Create a `syn::Expr` from an integer literal with a type suffix.
fn make_lit_int(repr: &str, _span: Span) -> syn::Expr {
    let lit: syn::LitInt = syn::parse_str(repr).expect("valid integer literal");
    syn::parse_quote! { #lit }
}

/// Create a `syn::Expr` from a float literal with a type suffix.
fn make_lit_float(repr: &str, _span: Span) -> syn::Expr {
    let lit: syn::LitFloat = syn::parse_str(repr).expect("valid float literal");
    syn::parse_quote! { #lit }
}
