use std::fmt::Display;

use quote::{format_ident, IdentFragment};
use syn::{parse::Parse, token::Comma, Expr, Ident, LitStr};

#[derive(Debug)]
pub(crate) struct HexName<'a>(pub &'a str, pub bool);

impl<'a> Display for HexName<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base = if self.1 { 65 } else { 97 };
        for b in self.0.bytes() {
            let high = (b >> 4) & 0xF;
            let low = b & 0xF;
            write!(f, "{}{}", (base + high) as char, (base + low) as char)?;
        }
        Ok(())
    }
}

impl<'a> IdentFragment for HexName<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StaticString(pub String);

impl StaticString {
    pub fn compile_name(&self) -> Ident {
        format_ident!("STATIC_STRING_{}", HexName(&self.0, true))
    }
}

impl Parse for StaticString {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let s: LitStr = input.parse()?;
        Ok(StaticString(s.value()))
    }
}

#[derive(Debug)]
pub struct Shutdown {
    pub msg: StaticString,
    pub clock: Expr,
}

impl Parse for Shutdown {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let msg = input.parse()?;
        input.parse::<Comma>()?;
        let clock = input.parse()?;
        Ok(Shutdown { msg, clock })
    }
}
