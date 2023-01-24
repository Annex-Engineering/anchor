use syn::{spanned::Spanned, Attribute, Error, Lit, LitStr, Meta, NestedMeta};

pub fn visit_attribs(
    attrs: &[Attribute],
    ident: &str,
    mut cb: impl FnMut(&NestedMeta) -> syn::Result<()>,
) -> syn::Result<()> {
    for mv in attrs
        .iter()
        .filter(|attr| attr.path.is_ident(ident))
        .map(|attr| match attr.parse_meta() {
            Ok(Meta::List(meta)) => Ok(meta.nested.into_iter().collect::<Vec<_>>()),
            Ok(other) => Err(Error::new(
                other.span(),
                format!("expected #[{ident}(...)]"),
            )),
            Err(err) => Err(err),
        })
    {
        for mv in mv? {
            cb(&mv)?;
        }
    }

    Ok(())
}

pub fn check_is_disabled(attrs: &[Attribute]) -> bool {
    fn check_expr(meta: &NestedMeta) -> bool {
        let v = match meta {
            NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("feature") => {
                if let Ok(feature) = get_lit_str(&m.lit) {
                    let feature = feature.value();
                    let envname =
                        format!("CARGO_FEATURE_{}", feature.to_uppercase().replace('-', "_"));
                    std::env::var(envname).is_ok()
                } else {
                    true
                }
            }
            NestedMeta::Meta(Meta::List(m)) if m.path.is_ident("not") => {
                let sub = m.nested.first().map_or(false, check_expr);
                !sub
            }
            NestedMeta::Meta(Meta::List(m)) if m.path.is_ident("all") => {
                m.nested.iter().all(check_expr)
            }
            NestedMeta::Meta(Meta::List(m)) if m.path.is_ident("any") => {
                m.nested.iter().any(check_expr)
            }
            _ => true,
        };
        v
    }

    let mut v = true;
    let _ = visit_attribs(attrs, "cfg", |m| {
        if v && !check_expr(m) {
            v = false;
        }
        Ok(())
    });
    !v
}

pub fn check_is_enabled(attrs: &[Attribute]) -> bool {
    !check_is_disabled(attrs)
}

pub fn get_lit_str(lit: &Lit) -> syn::Result<&LitStr> {
    if let Lit::Str(s) = lit {
        Ok(s)
    } else {
        Err(Error::new(lit.span(), "expected attribute to be a string"))
    }
}
