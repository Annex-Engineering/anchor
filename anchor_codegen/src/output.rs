use std::collections::BTreeMap;

use crate::static_string::HexName;
use quote::format_ident;
use syn::{parse::Parse, token::Comma, Expr, Ident, LitStr, Type};

#[derive(Debug, Eq, PartialEq)]
pub struct Output {
    pub id: Option<u8>,
    pub format: String,
    pub args: Vec<Arg>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Arg {
    pub type_: Type,
    pub value: Option<Expr>,
}

impl Output {
    pub fn sender_fn_name(&self) -> Ident {
        format_ident!("send_output_{}", HexName(&self.format, false))
    }

    pub fn clear_arg_values(&mut self) {
        for arg in self.args.iter_mut() {
            arg.value = None;
        }
    }
}

lazy_static::lazy_static! {
    static ref TYPE_MAP: BTreeMap<&'static str, &'static str> = BTreeMap::from([
        ("u", "u32"),
        ("i", "i32"),
        ("hu", "u16"),
        ("hi", "i16"),
        ("c", "u8"),
        (".*s", "&[u8]"),
        ("*s", "&str"),
    ]);
}

fn parse_args(mut fmt: &str) -> syn::Result<Vec<Arg>> {
    let mut args = vec![];
    while let Some(pos) = fmt.find('%') {
        fmt = &fmt[pos + 1..];
        for (kind, type_) in TYPE_MAP.iter() {
            if fmt.starts_with(kind) {
                let type_ = syn::parse_str(type_).unwrap();
                args.push(Arg { type_, value: None });
                break;
            }
        }
    }
    Ok(args)
}

impl Parse for Output {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let format = input.parse::<LitStr>()?.value();
        let mut args = parse_args(&format)?;

        for arg in args.iter_mut() {
            input.parse::<Comma>()?;
            arg.value = Some(input.parse()?);
        }

        if !input.is_empty() {
            Err(input.error("Unexpected extra arguments"))
        } else {
            Ok(Output {
                id: None,
                format,
                args,
            })
        }
    }
}
