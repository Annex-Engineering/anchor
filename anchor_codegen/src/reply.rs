use crate::msg_desc::{build_message_descriptor, DescArg};
use quote::format_ident;
use syn::{
    bracketed,
    parse::{Error, Parse, ParseStream, Result},
    token::{Bracket, Colon, Comma, Eq},
    Expr, Ident, LitInt, Type,
};

#[derive(Debug, Eq, PartialEq)]
pub struct Reply {
    pub name: Ident,
    pub id: Option<u8>,
    pub args: Vec<Arg>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Arg {
    pub name: Ident,
    pub type_: Type,
    pub value: Option<Expr>,
}

impl Reply {
    pub fn sender_fn_name(&self) -> Ident {
        format_ident!("send_reply_{}", self.name)
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

    pub fn clear_arg_values(&mut self) {
        for arg in self.args.iter_mut() {
            arg.value = None;
        }
    }
}

impl Parse for Reply {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        let mut id = None;

        // Check for attributes
        if input.peek(Bracket) {
            let content;
            bracketed!(content in input);
            let mut first = true;
            while !content.is_empty() {
                if !first {
                    input.parse::<Comma>()?;
                }
                first = false;
                let attrib_name: Ident = content.parse()?;
                content.parse::<Eq>()?;
                match attrib_name.to_string().as_str() {
                    "id" => {
                        id = Some(content.parse::<LitInt>()?.base10_parse()?);
                    }
                    _ => {
                        return Err(Error::new(
                            attrib_name.span(),
                            format!("Unknown attribute '{}'", attrib_name),
                        ))
                    }
                }
            }
        }

        let mut args = Vec::new();
        while !input.is_empty() {
            input.parse::<Comma>()?;
            let name = input.parse()?;
            input.parse::<Colon>()?;
            let type_ = input.parse()?;

            let value = if input.peek(Eq) {
                input.parse::<Eq>()?;
                Some(input.parse()?)
            } else {
                None
            };

            args.push(Arg { name, type_, value });
        }
        Ok(Reply { name, id, args })
    }
}
