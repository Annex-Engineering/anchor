//! Build step library for Anchor.
//! See the main library documentation for documentation on how to use Anchor.

use anyhow::Result;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use syn::{
    parse2,
    visit::{self, Visit},
    Ident, ItemConst, ItemFn, ItemMod, LitInt, LitStr, Macro,
};

#[doc(hidden)]
pub mod command;
#[doc(hidden)]
pub mod enumeration;
#[doc(hidden)]
pub mod generate;
#[doc(hidden)]
pub mod msg_desc;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod reply;
#[doc(hidden)]
pub mod static_string;
mod utils;

use crate::enumeration::{DictionaryEnumeration, DictionaryEnumerationItem, Enumeration};
use command::Command;
use generate::GenerateConfig;
use output::Output;
use reply::Reply;
use static_string::{Shutdown, StaticString};
use utils::*;

/// Build step for generating runtime functions and dictionary
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    entries: Vec<(PathBuf, Vec<Ident>)>,
    version: Option<String>,
    build_versions: Option<String>,
    skip_commands: BTreeSet<String>,
}

impl ConfigBuilder {
    /// Creates a new `ConfigBuilder`
    pub fn new() -> Self {
        ConfigBuilder::default()
    }

    /// Adds an entry point
    ///
    /// The builder will start from all supplied entries, parsing these modules and all their
    /// submodules.
    ///
    /// Generally this should be done only for the `src/main.rs` file of a project.
    pub fn entry(self, path: impl AsRef<Path>) -> Self {
        self.entry_module(path, &[])
    }

    /// Like `entry` but specifies the starting module path.
    ///
    /// For `src/main.rs` this is an empty list, and each submodule adds an element to the `module`
    /// path. E.g. `crate::module::submodule` would be `[module, submodule]`.
    ///
    /// Generally it should not be necessary to use this function.
    pub fn entry_module(mut self, path: impl AsRef<Path>, module: &[Ident]) -> Self {
        self.entries
            .push((path.as_ref().to_owned(), module.to_vec()));
        self
    }

    /// Sets the version string that will be placed in the dictionary
    pub fn set_version(mut self, version: impl AsRef<str>) -> Self {
        self.version = Some(version.as_ref().into());
        self
    }

    /// Sets the build version string that will be placed in the dictionary
    ///
    /// It is customary for this string to be formatted as a space separated list of pairs as so:
    /// ```text
    /// gcc: (15:7-2018-q2-6) 7.3.1 20180622 (release) [ARM/embedded-7-branch revision 261907] binutils: (2.31.1-11+rpi1+11) 2.31.1)
    /// ```
    ///
    /// Anchor does not enforce this, and allows the user to set any valid string value.
    pub fn set_build_versions(mut self, build_versions: impl AsRef<str>) -> Self {
        self.build_versions = Some(build_versions.as_ref().into());
        self
    }

    /// Ignores the `klipper_command` with a given name
    ///
    /// This can be used for disabling certain commands in specific builds. Generally it is
    /// preferable to use `#[cfg(feature)]` tags for disabling commands.
    pub fn skip_command(mut self, command: impl AsRef<str>) -> Self {
        self.skip_commands.insert(command.as_ref().into());
        self
    }

    /// Runs the build step
    pub fn build(self) {
        let mut processor = Processor {
            queue: self
                .entries
                .into_iter()
                .map(|(path, module_path)| Task { path, module_path })
                .collect(),
            errors: vec![],
            current_file: None,
            current_module: vec![],

            messages: BTreeMap::new(),
            static_strings: StaticStringsTracker::new(),
            dictionary: Dictionary::default(),
            generate_cfg: None,
        };

        if let Some(s) = self.version {
            processor.dictionary.version = s;
        }
        if let Some(s) = self.build_versions {
            processor.dictionary.build_versions = s;
        }

        processor.add_identify();
        if let Err(e) = processor.process_all() {
            if e.is::<syn::parse::Error>() {
                // We ignore parse errors as we'd like the user to see these
                // directly from the compiler. If we panic here, the compile
                // stage never starts.
                return;
            }
        }

        for cmd in self.skip_commands {
            processor.messages.remove(&cmd);
        }

        processor.assign_ids();
        processor.finalize_dictionary();

        // panic!("{:#?}", processor.dictionary);

        let outfile = format!(
            "{}/_anchor_config.rs",
            env::var("OUT_DIR").expect("could not get OUT_DIR")
        );
        let mut f = File::create(outfile).expect("Could not create output file");
        processor.write(&mut f).expect("Could not write config");
    }
}

#[derive(Debug)]
struct Task {
    path: PathBuf,
    module_path: Vec<Ident>,
}

#[derive(Debug, Eq, PartialEq)]
enum Message {
    Command(Command),
    Reply(Reply),
    Output(Output),
}

impl Message {
    fn id(&self) -> Option<u8> {
        match self {
            Message::Command(c) => c.id,
            Message::Reply(r) => r.id,
            Message::Output(o) => o.id,
        }
    }

    fn set_id(&mut self, id: Option<u8>) {
        match self {
            Message::Command(c) => c.id = id,
            Message::Reply(r) => r.id = id,
            Message::Output(o) => o.id = id,
        }
    }
}

#[derive(Debug)]
struct Processor {
    queue: VecDeque<Task>,
    errors: Vec<anyhow::Error>,
    current_file: Option<PathBuf>,
    current_module: Vec<Ident>,

    messages: BTreeMap<String, Message>,
    static_strings: StaticStringsTracker,
    dictionary: Dictionary,
    generate_cfg: Option<GenerateConfig>,
}

#[derive(Debug)]
struct StaticStringsTracker {
    strings: BTreeMap<StaticString, u16>,
    next_id: u16,
}

impl StaticStringsTracker {
    fn new() -> StaticStringsTracker {
        StaticStringsTracker {
            strings: BTreeMap::new(),
            next_id: 2, // STATIC_STRING_MIN
        }
    }

    fn insert(&mut self, ss: StaticString) {
        if self.strings.contains_key(&ss) {
            return;
        }
        self.strings.insert(ss, self.next_id);
        self.next_id += 1;
    }
}

#[derive(Debug, Serialize, Default)]
struct Dictionary {
    build_versions: String,
    version: String,

    config: BTreeMap<String, serde_json::Value>,
    commands: BTreeMap<String, u8>,
    responses: BTreeMap<String, u8>,
    output: BTreeMap<String, u8>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    enumerations: BTreeMap<String, DictionaryEnumeration>,
}

impl Dictionary {
    pub fn to_compressed(&self) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        serde_json::to_writer(&mut e, self).expect("Could not serialize data dictionary");
        e.finish().expect("Could not serialize data dictionary")
    }
}

macro_rules! check_error {
    ($self:ident, $expr:expr) => {
        if let Err(e) = $expr {
            $self.errors.push(e);
        }
    };
}

impl<'ast> Visit<'ast> for Processor {
    fn visit_macro(&mut self, node: &'ast Macro) {
        match path_last_name(&node.path).map(Ident::to_string).as_deref() {
            Some("klipper_static_string") => check_error!(self, self.process_static_string(node)),
            Some("klipper_shutdown") => check_error!(self, self.process_klipper_shutdown(node)),
            Some("klipper_reply") => check_error!(self, self.process_reply(node)),
            Some("klipper_output") => check_error!(self, self.process_output(node)),
            Some("klipper_enumeration") => check_error!(self, self.process_enumeration(node)),
            Some("klipper_config_generate") => {
                check_error!(self, self.process_config_generate(node))
            }
            _ => {}
        }

        visit::visit_macro(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        for attr in &node.attrs {
            if path_last_name(&attr.path).map_or(false, |i| i == "klipper_command") {
                check_error!(self, self.process_command(node));
                break;
            }
        }

        if check_is_enabled(&node.attrs) {
            visit::visit_item_fn(self, node);
        }
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        for attr in &node.attrs {
            if path_last_name(&attr.path).map_or(false, |i| i == "klipper_constant") {
                check_error!(self, self.process_constant(node));
                break;
            }
        }
        visit::visit_item_const(self, node)
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if check_is_disabled(&node.attrs) {
            return;
        }
        let pop = if node.content.is_some() {
            self.current_module.push(node.ident.clone());
            true
        } else {
            check_error!(self, self.queue_submodule(&node.ident));
            false
        };
        visit::visit_item_mod(self, node);
        if pop {
            self.current_module.pop();
        }
    }
}

impl Processor {
    fn process_all(&mut self) -> Result<()> {
        while let Some(next) = self.queue.pop_front() {
            self.process_one(next)?;
        }
        Ok(())
    }

    fn process_one(&mut self, task: Task) -> Result<()> {
        println!("cargo:rerun-if-changed={}", task.path.display());
        let content = std::fs::read_to_string(&task.path)?;
        let ast = syn::parse_file(&content)?;
        self.current_file = Some(task.path);
        self.current_module = task.module_path;
        self.visit_file(&ast);
        match self.errors.pop() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn queue_submodule(&mut self, name: &Ident) -> Result<()> {
        let base = self
            .current_file
            .as_ref()
            .and_then(|p| p.parent())
            .ok_or_else(|| anyhow::anyhow!("No current file?"))?;
        let mut module_path = self.current_module.clone();
        module_path.push(name.clone());

        // Ignore the module we are generating
        if module_path == ["_anchor_config"] {
            return Ok(());
        }

        let candidates: Vec<_> = [
            base.join(format!("{}.rs", name)),
            base.join(name.to_string()).join("mod.rs"),
        ]
        .into_iter()
        .filter(|p| p.exists())
        .collect();

        let file = match candidates.len() {
            2 => panic!(
                "Both {}.rs and {}/mod.rs exist. Remove one to break ambiguity.",
                name, name
            ),
            0 => panic!("Cannot find either {}.rs or {}/mod.rs", name, name),
            1 => &candidates[0],
            _ => unreachable!(),
        };
        self.queue.push_back(Task {
            path: file.to_owned(),
            module_path,
        });
        Ok(())
    }

    fn process_enumeration(&mut self, mac: &Macro) -> Result<()> {
        let enumeration = mac.parse_body::<Enumeration>()?;
        self.add_enum(enumeration.dictionary_name(), enumeration.to_dictionary());
        Ok(())
    }

    fn process_static_string(&mut self, mac: &Macro) -> Result<()> {
        let ss = mac.parse_body::<StaticString>()?;
        self.static_strings.insert(ss);
        Ok(())
    }

    fn process_klipper_shutdown(&mut self, mac: &Macro) -> Result<()> {
        let ss = mac.parse_body::<Shutdown>()?;
        self.static_strings.insert(ss.msg);
        if !self.messages.contains_key("shutdown") {
            self.add_message(
                "shutdown".into(),
                Message::Reply(Reply {
                    name: format_ident!("shutdown"),
                    id: None,
                    args: vec![
                        reply::Arg {
                            name: format_ident!("clock"),
                            type_: syn::parse_str("u32").unwrap(),
                            value: None,
                        },
                        reply::Arg {
                            name: format_ident!("static_string_id"),
                            type_: syn::parse_str("u16").unwrap(),
                            value: None,
                        },
                    ],
                }),
            );
        }
        Ok(())
    }

    fn process_command(&mut self, func: &ItemFn) -> Result<()> {
        let mut c = parse2::<Command>(func.to_token_stream())?;
        c.module = Some(self.current_module.clone());
        if check_is_enabled(&func.attrs) {
            self.add_message(c.name.to_string(), Message::Command(c));
        }
        Ok(())
    }

    fn process_reply(&mut self, mac: &Macro) -> Result<()> {
        let mut reply = parse2::<Reply>(mac.tokens.clone())?;
        reply.clear_arg_values();
        self.add_message(reply.name.to_string(), Message::Reply(reply));
        Ok(())
    }

    fn process_output(&mut self, mac: &Macro) -> Result<()> {
        let mut output = parse2::<Output>(mac.tokens.clone())?;
        output.clear_arg_values();
        self.add_message(output.format.to_string(), Message::Output(output));
        Ok(())
    }

    fn process_config_generate(&mut self, mac: &Macro) -> Result<()> {
        if self.generate_cfg.is_some() {
            return Err(anyhow::anyhow!(
                "Multiple klipper_config_generate calls found!"
            ));
        }
        self.generate_cfg = Some(parse2::<GenerateConfig>(mac.tokens.clone())?);
        Ok(())
    }

    fn process_constant(&mut self, node: &ItemConst) -> Result<()> {
        if check_is_disabled(&node.attrs) {
            return Ok(());
        }

        let name = node.ident.to_string();
        let expr = &node.expr;
        let value: serde_json::Value = if let Ok(v) = parse2::<LitInt>(expr.to_token_stream()) {
            v.base10_parse::<u32>()?.into()
        } else if let Ok(v) = parse2::<LitStr>(expr.to_token_stream()) {
            v.value().into()
        } else {
            panic!(
                "Can't understand constant {}, only types convertable to JSON are supported",
                name
            );
        };

        if self.dictionary.config.contains_key(&name) {
            panic!("Multiple definitions for klipper constant {}", name);
        }
        self.dictionary.config.insert(name, value);

        Ok(())
    }

    fn add_message(&mut self, name: String, message: Message) {
        if let Some(current) = self.messages.get(&name) {
            if current != &message {
                panic!("A command named {} already exists", name);
            }
        }
        self.messages.insert(name, message);
    }

    fn add_enum(&mut self, name: String, enumeration: DictionaryEnumeration) {
        if self.messages.contains_key(&name) {
            panic!("An enumeration named {} already exists", name);
        }
        self.dictionary.enumerations.insert(name, enumeration);
    }

    fn add_identify(&mut self) {
        self.add_message(
            "identify_response".into(),
            Message::Reply(Reply {
                name: format_ident!("identify_response"),
                id: Some(0),
                args: vec![
                    reply::Arg {
                        name: format_ident!("offset"),
                        type_: syn::parse_str("u32").unwrap(),
                        value: None,
                    },
                    reply::Arg {
                        name: format_ident!("data"),
                        type_: syn::parse_str("&[u8]").unwrap(),
                        value: None,
                    },
                ],
            }),
        );

        self.add_message(
            "identify".into(),
            Message::Command(Command {
                name: format_ident!("identify"),
                id: Some(1),
                module: None,
                handler_name: format_ident!("handle_identify"),
                has_context: false,
                args: vec![
                    command::Arg {
                        name: format_ident!("offset"),
                        type_: syn::parse_str("u32").unwrap(),
                    },
                    command::Arg {
                        name: format_ident!("count"),
                        type_: syn::parse_str("u32").unwrap(),
                    },
                ],
            }),
        );
    }

    fn assign_ids(&mut self) {
        self.assign_command_ids();
    }

    fn assign_command_ids(&mut self) {
        let mut used_ids = BTreeSet::new();
        for r in self.messages.values() {
            if let Some(id) = r.id() {
                used_ids.insert(id);
            }
        }

        let mut next_id = 0u8;
        let mut assign_id = || {
            let mut id = next_id;
            if id == 255 {
                panic!("Too many commands");
            }
            while used_ids.contains(&id) {
                id += 1;
            }
            used_ids.insert(id);
            next_id = id + 1;
            id
        };

        for c in self.messages.values_mut() {
            if c.id().is_none() {
                c.set_id(Some(assign_id()));
            }
        }
    }

    fn finalize_dictionary(&mut self) {
        for m in self.messages.values() {
            match m {
                Message::Command(c) => {
                    self.dictionary
                        .commands
                        .insert(c.get_desc_string(), c.id.unwrap());
                }
                Message::Reply(r) => {
                    self.dictionary
                        .responses
                        .insert(r.get_desc_string(), r.id.unwrap());
                }
                Message::Output(o) => {
                    self.dictionary
                        .output
                        .insert(o.format.clone(), o.id.unwrap());
                }
            }
        }
        let mut static_string_enum = BTreeMap::new();
        for (ss, idx) in &self.static_strings.strings {
            static_string_enum.insert(ss.0.clone(), DictionaryEnumerationItem::Number(*idx as i64));
        }
        self.dictionary.enumerations.insert(
            "static_string_id".to_string(),
            DictionaryEnumeration(static_string_enum),
        );
    }

    fn write(self, target: &mut impl Write) -> Result<()> {
        let dispatcher = self.write_message_dispatcher();
        let message_handlers = self.write_message_handlers();
        let static_string_ids = self.write_static_string_ids();
        let data_dictionary = self.write_data_dictionary();

        let cfg_opts = self.generate_cfg.as_ref().map(|cfg| {
            let (transport_name, transport_type) = &cfg.transport.as_ref().unwrap();
            let context = &cfg.context;
            quote! {
                use #transport_name;
                type Output = &'static #transport_type;
                type Context<'ctx> = #context;
            }
        });
        write!(
            target,
            "{}",
            quote! {
                #![allow(dead_code)]
                #![allow(unused_variables)]

                use ::anchor::{transport_output::TransportOutput, transport::Transport};
                pub mod message_handlers {
                    use super::*;
                    #(#message_handlers)*
                }
                pub mod static_strings {
                    #(#static_string_ids)*
                }

                #cfg_opts

                pub(crate) struct Config;

                impl ::anchor::transport::Config for Config {
                    type TransportOutput = Output;
                    type Context<'ctx> = Context<'ctx>;
                    #dispatcher
                }

                pub(crate) const CONFIG: Config = Config;
                pub(crate) static TRANSPORT: Transport<Config> = Transport::new(&CONFIG, &TRANSPORT_OUTPUT);

                #data_dictionary
            }
        )?;
        Ok(())
    }

    fn write_message_dispatcher(&self) -> TokenStream {
        let mut handlers = vec![None; 256];

        for m in self.messages.values() {
            let id = m.id().unwrap();
            if handlers[id as usize].is_some() {
                panic!("Multiple entries for command ID {}", id);
            }
            if let Message::Command(c) = m {
                let handler = c.handler_fn_name();
                handlers[id as usize] = Some(quote! {
                    #id => message_handlers::#handler(frame, context),
                });
            }
        }

        let handlers: Vec<_> = handlers.into_iter().flatten().collect();

        quote! {
            fn dispatch(cmd: u8, frame: &mut &[u8], context: &mut Context) -> Result<(), ::anchor::encoding::ReadError> {
                match cmd {
                    #(#handlers)*
                    _unknown_cmd => Err(::anchor::encoding::ReadError),
                }
            }
        }
    }

    fn write_message_handlers(&self) -> Vec<TokenStream> {
        self.messages
            .values()
            .map(|m| match m {
                Message::Command(c) => {
                    let handler_name = c.handler_fn_name();

                    let mut args = Vec::new();
                    let mut call_args = Vec::new();
                    for arg in &c.args {
                        let name = &arg.name;
                        let ty = &arg.type_;
                        args.push(quote! {
                            let #name = <#ty as ::anchor::encoding::Readable>::read(data)?;
                        });
                        call_args.push(name);
                    }

                    let target = c.target();
                    let ctx_arg = c.has_context.then(|| quote! {
                        context,
                    });
                    quote! {
                        #[allow(unused_variables)]
                        pub fn #handler_name(data: &mut &[u8], context: &mut Context) -> Result<(), ::anchor::encoding::ReadError> {
                            #(#args)*
                            #target(#ctx_arg #(#call_args),*);
                            Ok(())
                        }
                    }
                }
                Message::Reply(r) => {
                    let name = r.sender_fn_name();
                    let id = r.id.unwrap();

                    let args: Vec<_> = r
                        .args
                        .iter()
                        .map(|a| {
                            let name = &a.name;
                            let type_ = &a.type_;
                            quote! {
                                #name: #type_
                            }
                        })
                        .collect();

                    let writers: Vec<_> = r
                        .args
                        .iter()
                        .map(|a| {
                            let name = &a.name;
                            let type_ = &a.type_;
                            quote! {
                                <#type_ as ::anchor::encoding::Writable>::write(&#name, output);
                            }
                        })
                        .collect();

                    quote! {
                        pub fn #name ( #(#args),* ) {
                            TRANSPORT.encode_frame(|output: &mut <Output as TransportOutput>::Output| {
                                use ::anchor::OutputBuffer;
                                #[allow(unused_imports)]
                                use ::anchor::encoding::*;
                                output.output(&[#id]);
                                #(#writers)*
                            });
                        }
                    }
                }
                Message::Output(o) => {
                    let id = o.id.unwrap();
                    let name = o.sender_fn_name();

                    let args: Vec<_> = o
                        .args
                        .iter()
                        .enumerate()
                        .map(|(idx, a)| {
                            let name = format_ident!("arg_{}", idx);
                            let type_ = &a.type_;
                            quote! {
                                #name: #type_
                            }
                        })
                        .collect();

                    let writers: Vec<_> = o
                        .args
                        .iter()
                        .enumerate()
                        .map(|(idx,a)| {
                            let name = format_ident!("arg_{}", idx);
                            let type_ = &a.type_;
                            quote! {
                                <#type_ as ::anchor::encoding::Writable>::write(&#name, output);
                            }
                        })
                        .collect();

                    quote! {
                        pub fn #name ( #(#args),* ) {
                            TRANSPORT.encode_frame(|output: &mut <Output as TransportOutput>::Output| {
                                use ::anchor::OutputBuffer;
                                #[allow(unused_imports)]
                                use ::anchor::encoding::*;
                                output.output(&[#id]);
                                #(#writers)*
                            });
                        }
                    }
                }
            })
            .collect()
    }

    fn write_static_string_ids(&self) -> Vec<TokenStream> {
        self.static_strings
            .strings
            .iter()
            .map(|(ss, idx)| {
                let compile_name = ss.compile_name();
                quote! {
                    pub const #compile_name: u16 = #idx;
                }
            })
            .collect()
    }

    fn write_data_dictionary(&self) -> TokenStream {
        let data = self.dictionary.to_compressed();
        let len = data.len();
        quote! {
            const DATA: &[u8; #len] = &[#(#data),*];

            fn handle_identify(offset: u32, count: u32) {
                let end = (offset + count).min(DATA.len() as u32);
                let offset = offset.min(DATA.len() as u32);
                message_handlers::send_reply_identify_response(offset, &DATA[(offset as usize)..(end as usize)]);
            }
        }
    }
}

fn path_last_name(path: &syn::Path) -> Option<&Ident> {
    path.get_ident()
}
