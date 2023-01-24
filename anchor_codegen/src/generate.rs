use syn::{
    parse::{Error, Parse, ParseStream, Result},
    punctuated::Punctuated,
    token::{Colon, Comma, Eq, Paren},
    Ident, Path, Type, TypeTuple,
};

#[derive(Debug)]
pub struct GenerateConfig {
    pub transport: Option<(Path, Type)>,
    pub context: Type,
}

impl GenerateConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::bail;

        if self.transport.is_none() {
            bail!("Missing transport option");
        }

        Ok(())
    }
}

impl Parse for GenerateConfig {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut transport = None;
        let mut context = Type::Tuple(TypeTuple {
            paren_token: Paren { span: input.span() },
            elems: Punctuated::new(),
        });

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Eq>()?;

            match key.to_string().as_str() {
                "transport" => {
                    let name = input.parse()?;
                    input.parse::<Colon>()?;
                    let type_ = input.parse()?;
                    transport = Some((name, type_));
                }
                "context" => {
                    context = input.parse()?;
                }
                unkn => {
                    return Err(Error::new(
                        key.span(),
                        format!("Unknown attribute '{}'", unkn),
                    ))
                }
            }

            // Skip commas
            while input.parse::<Comma>().is_ok() {}
        }

        Ok(GenerateConfig { transport, context })
    }
}
