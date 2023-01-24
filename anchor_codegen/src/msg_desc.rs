use quote::ToTokens;
use std::collections::BTreeMap;
use syn::{Ident, Type};

pub struct DescArg<'a> {
    pub name: &'a Ident,
    pub type_: &'a Type,
}

lazy_static::lazy_static! {
    static ref TYPE_MAP: BTreeMap<&'static str, &'static str> = BTreeMap::from([
        ("u32", "%u"),
        ("i32", "%i"),
        ("& [u8]", "%*s"),
        ("bool", "%c"),
        ("u8", "%c"),
        ("u16", "%hu"),
        ("i16", "%hi"),
    ]);
}

pub fn build_message_descriptor<'a>(
    name: &Ident,
    args: impl Iterator<Item = DescArg<'a>>,
) -> String {
    use std::fmt::Write;
    let mut s = name.to_string();

    for a in args {
        let ty = a.type_.to_token_stream().to_string();
        let mapped = match TYPE_MAP.get(ty.as_str()) {
            Some(m) => m,
            None => panic!("Can't map type '{}' to a klipper data type", ty),
        };
        write!(s, " {}={}", a.name, mapped).unwrap();
    }

    s
}
