use serde::Serialize;
use std::{collections::BTreeMap, str::FromStr};

use crate::utils::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{
    braced, parenthesized, parse::Parse, parse_str, punctuated::Punctuated, spanned::Spanned,
    Attribute, Error, Ident, LitInt, Meta, NestedMeta, Token, Type, Visibility,
};

#[derive(Debug, Serialize)]
pub struct DictionaryEnumeration(pub BTreeMap<String, DictionaryEnumerationItem>);

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum DictionaryEnumerationItem {
    Number(i64),
    Range(i64, i64),
}

#[derive(Debug)]
pub struct Enumeration {
    opts: EnumerationOptions,
    attrs: Vec<Attribute>,
    ident: Ident,
    enum_token: Token![enum],
    visibility: Visibility,
    variants: Vec<EnumVariant>,
}

impl Enumeration {
    pub fn to_token_stream(&self) -> TokenStream {
        let attrs = self
            .attrs
            .iter()
            .filter(|a| !a.path.is_ident("klipper_enumeration"));
        let visibility = &self.visibility;
        let enum_token = &self.enum_token;
        let ident = &self.ident;
        let variant_decls = self.variants.iter().flat_map(Self::variant_decl);
        let variant_matches = self.variant_matches();
        let input_types = self.valid_input_types();
        let from_converters = input_types.iter().map(|typename| {
            let type_: Type = parse_str(typename).unwrap();
            quote! {
                impl core::convert::TryFrom<#type_> for #ident {
                    type Error = ();

                    fn try_from(value: #type_) -> Result<Self, Self::Error> {
                        match value {
                            #(#variant_matches)*
                            _ => Err(()),
                        }
                    }
                }
            }
        });
        let variant_to_matches = self.variant_to_matches();
        let to_converters = input_types.iter().map(|typename| {
            let type_: Type = parse_str(typename).unwrap();
            quote! {
                impl From<#ident> for #type_ {
                    fn from(value: #ident) -> #type_ {
                        match value {
                            #(#variant_to_matches)*
                        }
                    }
                }
            }
        });
        let max_variant = self.max_variant();

        quote! {
            #(#attrs)*
            #visibility #enum_token #ident {
                #(#variant_decls)*
            }

            impl #ident {
                fn max_variant() -> usize {
                    #max_variant
                }
            }

            #(#from_converters)*
            #(#to_converters)*
        }
    }

    fn variant_decl(variant: &EnumVariant) -> Vec<TokenStream> {
        match variant {
            EnumVariant::Single(opts, ident) => {
                let attrs = opts
                    .attrs
                    .iter()
                    .filter(|a| !a.path.is_ident("klipper_enumeration"))
                    .collect::<Vec<_>>();
                vec![quote! {
                    #(#attrs)*
                    #ident ,
                }]
            }
            EnumVariant::Range(opts, prefix, start, count) => (*start..*start + *count)
                .map(|i| {
                    let attrs = opts
                        .attrs
                        .iter()
                        .filter(|a| !a.path.is_ident("klipper_enumeration"))
                        .collect::<Vec<_>>();
                    let ident = format_ident!("{prefix}{i}");
                    quote! {
                        #(#attrs)*
                        #ident ,
                    }
                })
                .collect(),
        }
    }

    fn variant_matches(&self) -> Vec<TokenStream> {
        self.numbered_variants()
            .flat_map(|(v, start, cnt)| {
                let cfg_attrs = v.opts().attrs.iter().filter(|a| a.path.is_ident("cfg"));
                match v {
                    EnumVariant::Single(_, ident) => {
                        let start = TokenStream::from_str(&format!("{start}")).unwrap();
                        vec![quote! {
                            #(#cfg_attrs)*
                            #start => Ok(Self::#ident),
                        }]
                    }
                    EnumVariant::Range(_, prefix, ident_start, _) => {
                        let cfg_attrs = cfg_attrs.collect::<Vec<_>>();
                        (start..start + cnt)
                            .zip(*ident_start..*ident_start + cnt)
                            .map(|(i, n)| {
                                let ident = format_ident!("{prefix}{n}");
                                let i = TokenStream::from_str(&format!("{i}")).unwrap();
                                quote! {
                                    #(#cfg_attrs)*
                                    #i => Ok(Self::#ident),
                                }
                            })
                            .collect()
                    }
                }
            })
            .collect()
    }

    fn variant_to_matches(&self) -> Vec<TokenStream> {
        let self_ident = &self.ident;
        self.numbered_variants()
            .flat_map(|(v, start, cnt)| {
                let cfg_attrs = v.opts().attrs.iter().filter(|a| a.path.is_ident("cfg"));
                match v {
                    EnumVariant::Single(_, ident) => {
                        let start = TokenStream::from_str(&format!("{start}")).unwrap();
                        vec![quote! {
                            #(#cfg_attrs)*
                            #self_ident::#ident => #start,
                        }]
                    }
                    EnumVariant::Range(_, prefix, ident_start, _) => {
                        let cfg_attrs = cfg_attrs.collect::<Vec<_>>();
                        (*ident_start..*ident_start + cnt)
                            .map(|i| {
                                let ident = format_ident!("{prefix}{i}");
                                let i = TokenStream::from_str(&format!("{}", i + start)).unwrap();
                                quote! {
                                    #(#cfg_attrs)*
                                    #self_ident::#ident => #i,
                                }
                            })
                            .collect()
                    }
                }
            })
            .collect()
    }

    fn numbered_variants(&self) -> impl Iterator<Item = (&EnumVariant, usize, usize)> {
        self.variants.iter().scan(0, |state, variant| {
            let cnt = variant.count();
            let n = (variant, *state, cnt);
            *state += cnt;
            Some(n)
        })
    }

    fn max_variant(&self) -> usize {
        self.numbered_variants()
            .last()
            .map_or(0, |(_, s, c)| s + c - 1)
    }

    fn valid_input_types(&self) -> &'static [&'static str] {
        match self.max_variant() {
            0..=255 => &["u8", "u16", "u32", "u64", "usize"],
            256..=65535 => &["u16", "u32", "u64", "usize"],
            _ => &["u32", "u64", "usize"],
        }
    }

    pub fn dictionary_name(&self) -> String {
        self.opts
            .name
            .clone()
            .unwrap_or_else(|| self.ident.to_string())
    }

    pub fn to_dictionary(&self) -> DictionaryEnumeration {
        let mut out = BTreeMap::new();
        for (variant, start, cnt) in self.numbered_variants() {
            if variant.opts().disabled {
                continue;
            }
            match variant {
                EnumVariant::Single(_, _) => {
                    out.insert(
                        variant.name(self.opts.rename_all),
                        DictionaryEnumerationItem::Number(start as i64),
                    );
                }
                EnumVariant::Range(_, _, _, _) => {
                    out.insert(
                        variant.name(self.opts.rename_all),
                        DictionaryEnumerationItem::Range(start as i64, cnt as i64),
                    );
                }
            }
        }
        DictionaryEnumeration(out)
    }
}

impl Parse for Enumeration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility = input.parse::<Visibility>()?;
        let enum_token = input.parse::<Token!(enum)>()?;
        let ident = input.parse::<Ident>()?;

        let content;
        let _brace = braced!(content in input);
        let variants: Punctuated<EnumVariant, Token![,]> =
            content.parse_terminated(EnumVariant::parse)?;

        Ok(Enumeration {
            opts: EnumerationOptions::parse(&attrs)?,
            attrs,
            ident,
            enum_token,
            visibility,
            variants: variants.into_iter().collect(),
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RenameFormat {
    None,
    LowerCase,
    UpperCase,
    SnakeCase,
}

impl RenameFormat {
    pub fn apply(&self, s: &str) -> String {
        match self {
            Self::None => s.to_owned(),
            Self::LowerCase => s.to_lowercase(),
            Self::UpperCase => s.to_uppercase(),
            Self::SnakeCase => {
                let mut out = String::new();
                for (i, ch) in s.char_indices() {
                    if i > 0 && ch.is_uppercase() {
                        out.push('_');
                    }
                    out.push(ch.to_ascii_lowercase());
                }
                out
            }
        }
    }
}

impl Default for RenameFormat {
    fn default() -> Self {
        Self::None
    }
}

impl FromStr for RenameFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "lowercase" => Ok(Self::LowerCase),
            "UPPERCASE" => Ok(Self::UpperCase),
            "snake_case" => Ok(Self::SnakeCase),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Default)]
pub struct EnumerationOptions {
    name: Option<String>,
    rename_all: RenameFormat,
}

impl EnumerationOptions {
    fn parse(attrs: &[Attribute]) -> syn::Result<EnumerationOptions> {
        let mut opts = Self::default();

        visit_attribs(attrs, "klipper_enumeration", |meta| match meta {
            NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("name") => {
                opts.name = Some(get_lit_str(&m.lit)?.value());
                Ok(())
            }

            NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("rename_all") => {
                let format = get_lit_str(&m.lit)?.value();
                match format.parse() {
                    Ok(format) => {
                        opts.rename_all = format;
                        Ok(())
                    }
                    Err(()) => Err(Error::new(m.lit.span(), "unknown rename format")),
                }
            }

            NestedMeta::Meta(item) => Err(Error::new(
                item.span(),
                format!(
                    "unknown variant attribute '{}'",
                    item.path().into_token_stream().to_string().replace(' ', "")
                ),
            )),
            NestedMeta::Lit(lit) => Err(Error::new(
                lit.span(),
                "unexpected literal in variant attribute",
            )),
        })?;

        Ok(opts)
    }
}

#[derive(Debug)]
enum EnumVariant {
    Single(EnumVariantOpts, Ident),
    Range(EnumVariantOpts, Ident, usize, usize),
}

impl EnumVariant {
    pub fn opts(&self) -> &EnumVariantOpts {
        match self {
            Self::Single(opts, _) => opts,
            Self::Range(opts, _, _, _) => opts,
        }
    }

    fn count(&self) -> usize {
        match self {
            Self::Single(_, _) => 1,
            Self::Range(_, _, _, cnt) => *cnt,
        }
    }

    fn ident(&self) -> &Ident {
        match self {
            Self::Single(_, ident) => ident,
            Self::Range(_, ident, _, _) => ident,
        }
    }

    fn name(&self, rename_format: RenameFormat) -> String {
        if let Some(name) = self.opts().rename.as_ref() {
            name.clone()
        } else {
            rename_format.apply(&self.ident().to_string())
        }
    }
}

#[derive(Debug)]
struct EnumVariantOpts {
    disabled: bool,
    rename: Option<String>,
    attrs: Vec<Attribute>,
}

impl EnumVariantOpts {
    fn parse(attrs: Vec<Attribute>) -> syn::Result<EnumVariantOpts> {
        let disabled = check_is_disabled(&attrs);
        let mut opts = Self {
            attrs,
            disabled,
            rename: None,
        };

        if opts.disabled {
            return Ok(opts);
        }

        visit_attribs(&opts.attrs, "klipper_enumeration", |meta| match meta {
            NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("rename") => {
                opts.rename = Some(get_lit_str(&m.lit)?.value());
                Ok(())
            }

            NestedMeta::Meta(item) => Err(Error::new(
                item.span(),
                format!(
                    "unknown variant attribute '{}'",
                    item.path().into_token_stream().to_string().replace(' ', "")
                ),
            )),
            NestedMeta::Lit(lit) => Err(Error::new(
                lit.span(),
                "unexpected literal in variant attribute",
            )),
        })?;

        Ok(opts)
    }
}

impl Parse for EnumVariant {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let opts = EnumVariantOpts::parse(attrs)?;

        let _vis = input.parse::<Visibility>()?;
        let ident = input.parse::<Ident>()?;

        if ident == "Range" {
            let content;
            let _brace = parenthesized!(content in input);
            let prefix = content.parse()?;
            content.parse::<Token![,]>()?;
            let start = content.parse::<LitInt>()?.base10_parse()?;
            content.parse::<Token![,]>()?;
            let count = content.parse::<LitInt>()?.base10_parse()?;
            Ok(EnumVariant::Range(opts, prefix, start, count))
        } else {
            Ok(EnumVariant::Single(opts, ident))
        }
    }
}
