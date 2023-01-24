use crate::msg_desc::{build_message_descriptor, DescArg};
use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_str,
    token::Colon,
    Ident, ItemFn, PatIdent, PatType, Type,
};

#[derive(Debug, Eq, PartialEq)]
pub struct Arg {
    pub name: Ident,
    pub type_: Type,
}

impl Arg {
    fn new(name: Ident, type_: Type) -> Result<Arg> {
        let name = name.to_string();
        let name = parse_str::<Ident>(name.strip_prefix('_').unwrap_or(&name))?;
        Ok(Arg { name, type_ })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Command {
    pub name: Ident,
    pub id: Option<u8>,
    pub handler_name: Ident,
    pub module: Option<Vec<Ident>>,
    pub has_context: bool,
    pub args: Vec<Arg>,
}

impl Command {
    pub fn handler_fn_name(&self) -> Ident {
        format_ident!("_anchor_{}_handler", self.name)
    }

    pub fn target(&self) -> TokenStream {
        let hn = &self.handler_name;
        match &self.module {
            None => quote! { #hn },
            Some(mp) => {
                quote! {
                    crate:: #(#mp::)* #hn
                }
            }
        }
    }

    pub fn get_desc_string(&self) -> String {
        build_message_descriptor(
            &self.name,
            self.args.iter().map(|a| DescArg {
                name: &a.name,
                type_: &a.type_,
            }),
        )
    }
}

fn parse_has_context_param<'a>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a syn::FnArg)>>,
) -> bool {
    if let Some((_, syn::FnArg::Typed(PatType { pat, .. }))) = iter.peek() {
        if let syn::Pat::Ident(PatIdent { ident, .. }) = pat.as_ref() {
            let name = ident.to_string();
            let name = name.strip_prefix('_').unwrap_or(&name);
            if name == "context" || name == "ctx" {
                let _ = iter.next();
                return true;
            }
        }
    }
    false
}

impl Parse for Command {
    fn parse(input: ParseStream) -> Result<Self> {
        let func: ItemFn = input.parse()?;

        let mut inputs = func.sig.inputs.iter().enumerate().peekable();

        let has_context = parse_has_context_param(&mut inputs);

        let mut args = Vec::new();
        for (idx, arg) in inputs {
            match arg {
                syn::FnArg::Typed(PatType {
                    pat,
                    colon_token: Colon { .. },
                    ty,
                    ..
                }) => match pat.as_ref() {
                    syn::Pat::Ident(PatIdent { ident, .. }) => {
                        args.push(Arg::new(ident.clone(), ty.as_ref().clone())?);
                    }
                    _ => abort!("Argument {} has non-identifier name", idx),
                },
                _ => abort!("Could not understand argument {}", idx),
            }
        }

        let name = func.sig.ident;

        Ok(Command {
            name: name.clone(),
            module: None,
            handler_name: name,
            id: None,
            has_context,
            args,
        })
    }
}
