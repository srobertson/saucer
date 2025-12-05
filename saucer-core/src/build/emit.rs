use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};

use crate::build::ast::{ManagerInfo, PortSpec, RuntimeSpec, TemplateInfo};
use crate::build::hygiene::crate_to_module_name;
use crate::build::ports;
use crate::build::request::{generate_request_enum, generate_self_msg_type};

/// Produce the final generated runtime source as a string.
pub fn generate_runtime_source(spec: &RuntimeSpec) -> String {
    let manager_imports = generate_manager_imports(&spec.effect_managers);
    let request_enum = generate_request_enum(&spec.effect_managers, &spec.all_ports);
    let self_msg_type = generate_self_msg_type(&spec.effect_managers);
    let cmd_type = generate_cmd_type();
    let command_helpers =
        generate_command_helpers(&spec.effect_managers, &spec.used_helpers, &spec.all_ports);
    let runtime_state = generate_runtime_state(&spec.effect_managers);
    // Derive the app message path. Ports only exist on the root template, so prefer them;
    // otherwise fall back to the first template module (root) to avoid self-recursive aliases.
    let msg_ty: TokenStream =
        if let Some((_, crate_ident, module_ident)) = spec.ports_with_paths.first() {
            if crate_ident.to_string() == crate_to_module_name(&spec.package_name) {
                quote! { crate::runtime::#crate_ident::#module_ident::Msg }
            } else {
                quote! { ::#crate_ident::#module_ident::Msg }
            }
        } else if let Some(template) = spec.templates.first() {
            let crate_ident = format_ident!("{}", crate_to_module_name(&template.crate_name));
            let module_ident = format_ident!("{}", template.module_name);
            if crate_ident.to_string() == crate_to_module_name(&spec.package_name) {
                quote! { crate::runtime::#crate_ident::#module_ident::Msg }
            } else {
                quote! { ::#crate_ident::#module_ident::Msg }
            }
        } else {
            quote! { AppMsg }
        };

    let ports_struct = generate_ports_struct(
        &spec.ports_with_paths,
        &msg_ty,
        &crate_to_module_name(&spec.package_name),
    );
    let runtime = generate_runtime_struct(
        &spec.effect_managers,
        &spec.reconciler_manager,
        &spec.ports_with_paths,
        spec.has_outgoing_ports,
        &msg_ty,
    );

    let template_modules = build_template_modules(
        &spec.templates,
        &spec.transformed_templates,
        &spec.effect_managers,
    );

    let runtime_module = quote! {
        #[allow(unused_imports)]
        pub mod sync {
            use super::*;
            use saucer_core::{Observation, ObserverFn};
            #runtime
        }
    };

    let output_tokens = quote! {
        // Generated runtime module - do not edit manually

        #manager_imports
        #request_enum
        #self_msg_type
        #cmd_type
        #command_helpers
        #runtime_state
        #ports_struct
        #runtime_module
        #(#template_modules)*
    };

    let parsed: syn::File = syn::parse2(output_tokens).expect("Generated runtime did not parse");
    prettyplease::unparse(&parsed)
}

fn build_template_modules(
    templates: &[TemplateInfo],
    transformed_templates: &[(String, String)],
    managers: &[ManagerInfo],
) -> Vec<TokenStream> {
    let mut grouped: BTreeMap<String, Vec<TokenStream>> = BTreeMap::new();
    for (template, (_name, code)) in templates.iter().zip(transformed_templates.iter()) {
        let crate_mod = crate_to_module_name(&template.crate_name);
        let outgoing: Vec<PortSpec> = template
            .ports
            .iter()
            .cloned()
            .filter(|p| matches!(p, PortSpec::Outgoing(_)))
            .collect();
        let module_ts =
            generate_template_module_with_source(template, code.clone(), &outgoing, managers);
        grouped.entry(crate_mod).or_default().push(module_ts);
    }

    grouped
        .into_iter()
        .map(|(crate_mod, mods)| {
            let crate_ident = format_ident!("{}", crate_mod);
            quote! {
                pub mod #crate_ident {
                    #(#mods)*
                }
            }
        })
        .collect()
}

fn generate_manager_imports(_managers: &[ManagerInfo]) -> TokenStream {
    // All manager items are referenced with fully-qualified paths (`crate_name::Type`),
    // so we don't need glob imports that trigger unused-import warnings.
    quote! {}
}

fn generate_cmd_type() -> TokenStream {
    let cmd_ext = quote! {
        #[allow(dead_code)]
        pub trait CmdExt<Msg> {
            #[allow(dead_code)]
            fn map<Msg2>(self, f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static) -> Cmd<Msg2>;
        }

        #[allow(dead_code)]
        impl<Msg: 'static> CmdExt<Msg> for Cmd<Msg> {
            #[allow(dead_code)]
            fn map<Msg2>(self, f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static) -> Cmd<Msg2> {
                saucer_core::CoreCmd(
                    self.into_inner()
                        .into_iter()
                        .map(|req| req.map(f.clone()))
                        .collect()
                )
            }
        }
    };

    quote! {
        pub type Cmd<Msg> = saucer_core::CoreCmd<Request<Msg>>;
        #cmd_ext
    }
}

fn generate_command_helpers(
    managers: &[ManagerInfo],
    used_helpers: &[(String, String)],
    ports: &[PortSpec],
) -> TokenStream {
    // Core helpers live under `core::` only if used
    let core_mod = if used_helpers
        .iter()
        .any(|(m, h)| m == "saucer_core" && h == "shutdown")
    {
        quote! {
            pub mod core {
                use super::{Cmd, Request};
                pub fn shutdown<Msg>() -> Cmd<Msg> {
                    saucer_core::CoreCmd::single(Request::Core(saucer_core::shutdown()))
                }
            }
        }
    } else {
        quote! {}
    };

    let manager_modules: Vec<_> = managers
        .iter()
        .map(|m| {
            let module_ident = format_ident!("{}", m.module_name);
            let helpers = generate_typed_helpers_for_manager(m, used_helpers);
            if helpers.is_empty() {
                quote! {}
            } else {
                quote! {
                    pub mod #module_ident {
                        use super::*;
                        #helpers
                    }
                }
            }
        })
        .collect();

    let port_helpers: Vec<_> = ports
        .iter()
        .filter_map(|p| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let fn_ident = format_ident!("{}", p.name);
            let payload_ty = ports::payload_type_tokens(&PortSpec::Outgoing(p.clone()));
            let params: Vec<_> = p
                .args
                .iter()
                .map(|(name, ty)| {
                    let ident = format_ident!("{}", name);
                    let ty_ts: TokenStream = ty.parse().unwrap();
                    quote! { #ident: #ty_ts }
                })
                .collect();

            let call_args: Vec<_> = p
                .args
                .iter()
                .map(|(name, _)| format_ident!("{}", name))
                .collect();

            let variant_ident = format_ident!("{}", ports::to_camel(&p.name));
            let value_binding = if p.args.is_empty() {
                quote! { let value: #payload_ty = (); }
            } else if p.args.len() == 1 {
                let arg = call_args.first().unwrap();
                quote! { let value: #payload_ty = #arg; }
            } else {
                quote! { let value: #payload_ty = ( #(#call_args),* ); }
            };

            quote! {
                pub fn #fn_ident<Msg>(#(#params),*) -> Cmd<Msg> {
                    #value_binding
                    saucer_core::CoreCmd::single(Request::Ports(PortsRequest::#variant_ident { value }, std::marker::PhantomData))
                }
            }
        })
        .collect();

    quote! {
        #core_mod
        #(#manager_modules)*
        #(#port_helpers)*
    }
}

fn generate_typed_helpers_for_manager(
    manager: &ManagerInfo,
    used_helpers: &[(String, String)],
) -> TokenStream {
    let module_ident = format_ident!("{}", manager.module_name);
    let variant_ident = format_ident!("{}", manager.variant);
    let needed_for_mgr: Vec<String> = used_helpers
        .iter()
        .filter(|(mgr, _)| mgr == &manager.module_name)
        .map(|(_, h)| h.clone())
        .collect();

    let mut helper_tokens = Vec::new();
    let mut import_idents = Vec::new();

    for helper_name in needed_for_mgr {
        let (helper_ts, needed) =
            parse_and_generate_helper(manager, &helper_name, &module_ident, &variant_ident);
        helper_tokens.push(helper_ts);
        import_idents.extend(needed);
    }

    let mut seen = HashSet::new();
    let import_idents: Vec<_> = import_idents
        .into_iter()
        .filter(|id| seen.insert(id.to_string()))
        .collect();

    let imports = if import_idents.is_empty() {
        quote! {}
    } else {
        quote! { use ::#module_ident::{ #(#import_idents),* }; }
    };

    quote! {
        #imports
        #(#helper_tokens)*
    }
}

fn parse_and_generate_helper(
    manager: &ManagerInfo,
    helper_name: &str,
    module_ident: &proc_macro2::Ident,
    variant_ident: &proc_macro2::Ident,
) -> (TokenStream, Vec<syn::Ident>) {
    use syn::{FnArg, Item, ItemFn, Pat, ReturnType};

    // Enforce a consistent effect-manager structure: helpers must live in requests.rs
    // (breadcrumb command.rs is for fictional imports only). We no longer scan lib.rs
    // for helpers to avoid surprises and keep codegen deterministic.
    let requests_path = manager
        .lib_path
        .parent()
        .expect("lib.rs has a parent")
        .join("requests.rs");

    let req_source = std::fs::read_to_string(&requests_path).unwrap_or_else(|e| {
        panic!(
            "Helper `{}` must be declared in requests.rs; failed to read {:?}: {}",
            helper_name, requests_path, e
        )
    });
    let file_ast: syn::File = syn::parse_str(&req_source)
        .unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", requests_path, e));

    let sources: Vec<(PathBuf, syn::File)> = vec![(requests_path, file_ast)];

    let func: ItemFn = sources
        .iter()
        .find_map(|(path, file)| {
            file.items.iter().find_map(|item| match item {
                Item::Fn(f)
                    if f.sig.ident == helper_name
                        && matches!(f.vis, syn::Visibility::Public(_)) =>
                {
                    Some((f.clone(), path))
                }
                _ => None,
            })
        })
        .map(|(f, _)| f)
        .unwrap_or_else(|| {
            panic!(
                "Helper `{}` not found as pub fn in {:?} (lib.rs or requests.rs).",
                helper_name, manager.lib_path
            )
        });

    // Ensure return type matches the manager's request_type<Msg>
    match &func.sig.output {
        ReturnType::Type(_, ty) => {
            if !return_type_matches_request(ty, &manager.request_type) {
                panic!(
                    "Helper `{}` in `{}` must return `{}`; found `{}`.",
                    helper_name,
                    manager.crate_name,
                    manager.request_type,
                    ty.to_token_stream()
                );
            }
        }
        ReturnType::Default => panic!(
            "Helper `{}` in `{}` must return a request type.",
            helper_name, manager.crate_name
        ),
    }

    // Collect inputs and arg idents
    let mut params = Vec::new();
    let mut args = Vec::new();
    let mut type_collector = TypeIdentCollector::default();
    for arg in &func.sig.inputs {
        match arg {
            FnArg::Receiver(_) => panic!(
                "Helper `{}` in `{}` should not take self.",
                helper_name, manager.crate_name
            ),
            FnArg::Typed(pat_type) => {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let ty = &pat_type.ty;
                    type_collector.collect_type_idents(ty);
                    params.push(quote! { #pat_ident : #ty });
                    let ident = &pat_ident.ident;
                    args.push(quote! { #ident });
                } else {
                    panic!("Helper `{}` has unsupported pattern argument.", helper_name);
                }
            }
        }
    }

    // Ensure Msg generic exists
    if !func.sig.generics.type_params().any(|tp| tp.ident == "Msg") {
        panic!(
            "Helper `{}` in `{}` must be generic over Msg to generate Cmd<Msg>.",
            helper_name, manager.crate_name
        );
    }

    let fn_ident = format_ident!("{}", helper_name);
    let generics = &func.sig.generics;
    let where_clause = &func.sig.generics.where_clause;

    let needed_idents = type_collector.into_idents_except(&["Msg"]);

    let module_path = quote! { ::#module_ident };
    let helper_tokens = quote! {
        pub fn #fn_ident #generics (#(#params),*) -> Cmd<Msg> #where_clause {
            saucer_core::CoreCmd::single(Request::#variant_ident(#module_path::#fn_ident(#(#args),*)))
        }
    };

    (helper_tokens, needed_idents)
}

#[derive(Default)]
struct TypeIdentCollector {
    idents: Vec<syn::Ident>,
}

impl TypeIdentCollector {
    fn collect_type_idents(&mut self, ty: &syn::Type) {
        use syn::{
            GenericArgument, PathArguments, ReturnType as SynReturnType, Type, TypeParamBound,
            TypePath,
        };
        match ty {
            Type::Path(TypePath { qself: None, path }) => {
                if let Some(first) = path.segments.first() {
                    self.idents.push(first.ident.clone());
                }
                for segment in &path.segments {
                    match &segment.arguments {
                        PathArguments::AngleBracketed(args) => {
                            for arg in &args.args {
                                if let GenericArgument::Type(t) = arg {
                                    self.collect_type_idents(t);
                                }
                            }
                        }
                        PathArguments::Parenthesized(paren) => {
                            for input in &paren.inputs {
                                self.collect_type_idents(input);
                            }
                            if let SynReturnType::Type(_, ret_ty) = &paren.output {
                                self.collect_type_idents(ret_ty);
                            }
                        }
                        PathArguments::None => {}
                    }
                }
            }
            Type::Reference(r) => self.collect_type_idents(&r.elem),
            Type::Tuple(t) => {
                for elem in &t.elems {
                    self.collect_type_idents(elem);
                }
            }
            Type::Paren(p) => self.collect_type_idents(&p.elem),
            Type::ImplTrait(it) => {
                for b in &it.bounds {
                    if let TypeParamBound::Trait(tb) = b {
                        self.collect_trait_bound_idents(tb);
                    }
                }
            }
            Type::TraitObject(to) => {
                for b in &to.bounds {
                    if let TypeParamBound::Trait(tb) = b {
                        self.collect_trait_bound_idents(tb);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_trait_bound_idents(&mut self, tb: &syn::TraitBound) {
        use syn::{GenericArgument, PathArguments, ReturnType as SynReturnType};
        if let Some(first) = tb.path.segments.first() {
            self.idents.push(first.ident.clone());
        }
        for segment in &tb.path.segments {
            match &segment.arguments {
                PathArguments::AngleBracketed(args) => {
                    for arg in &args.args {
                        if let GenericArgument::Type(t) = arg {
                            self.collect_type_idents(t);
                        }
                    }
                }
                PathArguments::Parenthesized(paren) => {
                    for input in &paren.inputs {
                        self.collect_type_idents(input);
                    }
                    if let SynReturnType::Type(_, ret_ty) = &paren.output {
                        self.collect_type_idents(ret_ty);
                    }
                }
                PathArguments::None => {}
            }
        }
    }

    fn into_idents_except(self, skip: &[&str]) -> Vec<syn::Ident> {
        let mut skip_set: HashSet<String> = skip.iter().map(|s| s.to_string()).collect();
        // Standard/prelude types and traits we should not try to import from the manager crate
        let std_names = [
            "Into",
            "From",
            "Fn",
            "FnOnce",
            "FnMut",
            "Send",
            "Sync",
            "Clone",
            "Copy",
            "Debug",
            "Display",
            "String",
            "Result",
            "Option",
            "Vec",
            "Box",
            "Borrow",
            "AsRef",
            "PartialEq",
            "Eq",
            "PartialOrd",
            "Ord",
            "Hash",
            "Iterator",
            // primitives
            "f64",
            "f32",
            "i64",
            "i32",
            "i16",
            "i8",
            "u64",
            "u32",
            "u16",
            "u8",
            "isize",
            "usize",
            "bool",
            "char",
            "str",
        ];
        skip_set.extend(std_names.iter().map(|s| s.to_string()));
        let mut seen = HashSet::new();
        self.idents
            .into_iter()
            .filter(|id| {
                let name = id.to_string();
                !skip_set.contains(&name) && seen.insert(name)
            })
            .collect()
    }
}

fn return_type_matches_request(ty: &syn::Type, request_ident: &str) -> bool {
    if let syn::Type::Path(tp) = ty {
        if let Some(last) = tp.path.segments.last() {
            return last.ident == request_ident;
        }
    }
    false
}

fn generate_runtime_state(_managers: &[ManagerInfo]) -> TokenStream {
    quote! {}
}

fn generate_ports_struct(
    ports_with_paths: &[(PortSpec, proc_macro2::Ident, proc_macro2::Ident)],
    msg_ty: &TokenStream,
    local_crate: &str,
) -> TokenStream {
    if ports_with_paths.is_empty() {
        return quote! {};
    }

    // Define per-port structs
    let incoming_structs: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Incoming(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let struct_ident = format_ident!("{}PortIn", ports::to_camel(&p.name));
            let send_args: Vec<_> = p
                .args
                .iter()
                .map(|(n, t)| {
                    let ident = format_ident!("{}", n);
                    let ty: TokenStream = t.parse().unwrap();
                    quote! { #ident: #ty }
                })
                .collect();
            let call_idents: Vec<_> = p.args.iter().map(|(n, _)| format_ident!("{}", n)).collect();
            let send_sig = if send_args.is_empty() {
                quote! {
                    pub fn send(&self) {
                        let app_msg: AppMsg = (self.constructor)().into();
                        self.router.send_to_app(app_msg);
                    }
                }
            } else {
                quote! {
                    pub fn send(&self, #(#send_args),*) {
                        let app_msg: AppMsg = (self.constructor)(#(#call_idents),*).into();
                        self.router.send_to_app(app_msg);
                    }
                }
            };
            let fn_type: TokenStream = {
                let tys: Vec<_> = p
                    .args
                    .iter()
                    .map(|(_, t)| t.parse::<TokenStream>().unwrap())
                    .collect();
                if tys.is_empty() {
                    quote! { fn() -> #msg_ty }
                } else {
                    quote! { fn(#(#tys),*) -> #msg_ty }
                }
            };
            quote! {
                #[derive(Clone)]
                pub struct #struct_ident<SelfMsg, AppMsg>
                where
                    SelfMsg: Send + Clone + 'static,
                    AppMsg: Clone + Send + 'static + From<#msg_ty>,
                {
                    router: saucer_core::Router<AppMsg, SelfMsg>,
                    constructor: #fn_type,
                }
                impl<SelfMsg, AppMsg> #struct_ident<SelfMsg, AppMsg>
                where
                    SelfMsg: Send + Clone + 'static,
                    AppMsg: Clone + Send + 'static + From<#msg_ty>,
                {
                    #send_sig
                }
            }
        })
        .collect();

    let outgoing_structs: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let struct_ident = format_ident!("{}PortOut", ports::to_camel(&p.name));
            let inner_ident = format_ident!("{}PortOutInner", ports::to_camel(&p.name));
            let ty = ports::payload_type_tokens(&PortSpec::Outgoing(p.clone()));
            quote! {
                #[derive(Clone)]
                pub struct #struct_ident
                where
                    #ty: Send + Clone + 'static,
                {
                    inner: std::sync::Arc<#inner_ident>,
                }

                struct #inner_ident {
                    tx: tokio::sync::mpsc::UnboundedSender<#ty>,
                    subscribers: std::sync::Mutex<Vec<Box<dyn Fn(#ty) + Send + 'static>>>,
                }

                impl #struct_ident
                where
                    #ty: Send + Clone + 'static,
                {
                    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<#ty>) {
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<#ty>();
                        (
                            Self {
                                inner: std::sync::Arc::new(#inner_ident {
                                    tx,
                                    subscribers: std::sync::Mutex::new(Vec::new()),
                                }),
                            },
                            rx,
                        )
                    }

                    pub fn subscribe<F>(&self, f: F)
                    where
                        F: Fn(#ty) + Send + 'static,
                    {
                        self.inner
                            .subscribers
                            .lock()
                            .expect("port subscriber mutex poisoned")
                            .push(Box::new(f));
                    }

                    pub fn dispatch(&self, v: #ty) {
                        let _ = self.inner.tx.send(v);
                    }

                    pub fn deliver(&self, v: #ty) {
                        let subs = self.inner
                            .subscribers
                            .lock()
                            .expect("port subscriber mutex poisoned");
                        for handler in subs.iter() {
                            handler(v.clone());
                        }
                    }
                }

                impl Default for #struct_ident
                where
                    #ty: Send + Clone + 'static,
                {
                    fn default() -> Self {
                        let (port, _) = Self::new();
                        port
                    }
                }
            }
        })
        .collect();

    // Ports struct fields
    let incoming_fields: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Incoming(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}", p.name);
            let ty = format_ident!("{}PortIn", ports::to_camel(&p.name));
            quote! { pub #field: #ty<SelfMsg, AppMsg>, }
        })
        .collect();

    let outgoing_fields: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}", p.name);
            let ty = format_ident!("{}PortOut", ports::to_camel(&p.name));
            quote! { pub #field: #ty, }
        })
        .collect();

    let outgoing_field_inits: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}", p.name);
            quote! { #field, }
        })
        .collect();

    let outgoing_receiver_fields: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}_rx", p.name);
            let ty: TokenStream = ports::payload_type_tokens(&PortSpec::Outgoing(p.clone()));
            quote! { pub #field: tokio::sync::mpsc::UnboundedReceiver<#ty>, }
        })
        .collect();

    // Ports struct initializers
    let incoming_inits: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, crate_ident, module_ident)| match p {
            PortSpec::Incoming(p) => Some((p, crate_ident, module_ident)),
            _ => None,
        })
        .map(|(p, crate_ident, module_ident)| {
            let field = format_ident!("{}", p.name);
            let ty = format_ident!("{}PortIn", ports::to_camel(&p.name));
            let original_fn = if crate_ident.to_string() == local_crate {
                quote! { crate::#module_ident::#field }
            } else {
                quote! { ::#crate_ident::#module_ident::#field }
            };
            quote! {
                #field: #ty {
                    router: router.clone(),
                    constructor: #original_fn,
                },
            }
        })
        .collect();

    let outgoing_inits: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}", p.name);
            let ty = format_ident!("{}PortOut", ports::to_camel(&p.name));
            let rx_field = format_ident!("{}_rx", p.name);
            quote! { let (#field, #rx_field) = #ty::new(); }
        })
        .collect();

    let outgoing_receiver_inits: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field = format_ident!("{}_rx", p.name);
            quote! { #field, }
        })
        .collect();

    quote! {
        #(#incoming_structs)*
        #(#outgoing_structs)*

        #[derive(Clone)]
        pub struct Ports<SelfMsg, AppMsg>
        where
            SelfMsg: Send + Clone + 'static,
            AppMsg: Clone + Send + 'static + From<#msg_ty>,
        {
            #(#incoming_fields)*
            #(#outgoing_fields)*
        }

        impl<SelfMsg, AppMsg> Ports<SelfMsg, AppMsg>
        where
            SelfMsg: Send + Clone + 'static,
            AppMsg: Clone + Send + 'static + From<#msg_ty>,
        {
            fn new(router: saucer_core::Router<AppMsg, SelfMsg>) -> (Self, PortsReceivers) {
                #(#outgoing_inits)*

                let ports = Self {
                    #(#incoming_inits)*
                    #(#outgoing_field_inits)*
                };

                let receivers = PortsReceivers {
                    #(#outgoing_receiver_inits)*
                };

                (ports, receivers)
            }
        }

        pub struct PortsReceivers {
            #(#outgoing_receiver_fields)*
        }
    }
}

fn generate_runtime_struct(
    effect_managers: &[ManagerInfo],
    reconciler_manager: &ManagerInfo,
    ports_with_paths: &[(PortSpec, proc_macro2::Ident, proc_macro2::Ident)],
    has_outgoing_ports: bool,
    msg_ty: &TokenStream,
) -> TokenStream {
    let has_ports = !ports_with_paths.is_empty();

    let router_decls: Vec<_> = effect_managers
        .iter()
        .map(|m| {
            let router_ident = format_ident!("{}_router", m.variant.to_lowercase());
            quote! { let #router_ident = saucer_core::Router::new(self.app_tx.clone(), self.self_tx.clone()); }
        })
        .collect();

    let manager_init: Vec<_> = effect_managers
        .iter()
        .map(|m| {
            let manager_ident = format_ident!("{}_manager", m.variant.to_lowercase());
            let state_ident = format_ident!("{}_state", m.variant.to_lowercase());
            let module_ident = format_ident!("{}", m.module_name);
            let manager_type = format_ident!("{}", m.manager_type);
            quote! {
                let #manager_ident = ::#module_ident::#manager_type;
                let mut #state_ident = ::#module_ident::#manager_type::init();
            }
        })
        .collect();

    let request_dispatch: Vec<_> = effect_managers
        .iter()
        .map(|m| {
            let variant_ident = format_ident!("{}", m.variant);
            let router_ident = format_ident!("{}_router", m.variant.to_lowercase());
            let manager_ident = format_ident!("{}_manager", m.variant.to_lowercase());
            let state_ident = format_ident!("{}_state", m.variant.to_lowercase());
            quote! {
                Request::#variant_ident(r) => {
                    #state_ident = #manager_ident.on_effects(&#router_ident, #state_ident, vec![r]);
                }
            }
        })
        .collect();

    let self_msg_dispatch: Vec<_> = effect_managers
        .iter()
        .filter(|m| m.self_msg_type != "()")
        .map(|m| {
            let variant_ident = format_ident!("{}", m.variant);
            let router_ident = format_ident!("{}_router", m.variant.to_lowercase());
            let manager_ident = format_ident!("{}_manager", m.variant.to_lowercase());
            let state_ident = format_ident!("{}_state", m.variant.to_lowercase());
            let manager_label = m.variant.clone();
            quote! {
                SelfMsg::#variant_ident(msg) => {
                    let observation = Observation::ManagerMsg {
                        ts: std::time::SystemTime::now(),
                        manager: #manager_label,
                        data: SelfMsg::#variant_ident(msg.clone()),
                    };
                    observer(&observation);
                    #state_ident = #manager_ident.on_self_msg(#state_ident, &#router_ident, msg);
                }
            }
        })
        .collect();

    let self_msg_dispatch_arm = if self_msg_dispatch.is_empty() {
        quote! {}
    } else {
        quote! {
            Some(self_msg) = self.self_rx.recv() => {
                match self_msg {
                    #(#self_msg_dispatch)*
                }
            }
        }
    };

    let ports_match_arms: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let variant_ident = format_ident!("{}", ports::to_camel(&p.name));
            let field_ident = format_ident!("{}", p.name);
            quote! { PortsRequest::#variant_ident { value } => self.ports.#field_ident.dispatch(value), }
        })
        .collect();

    let outgoing_receiver_fields_struct: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field_ident = format_ident!("{}_rx", p.name);
            let ty: TokenStream = ports::payload_type_tokens(&PortSpec::Outgoing(p.clone()));
            quote! { #field_ident: tokio::sync::mpsc::UnboundedReceiver<#ty>, }
        })
        .collect();

    let outgoing_receiver_init_fields: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let field_ident = format_ident!("{}_rx", p.name);
            quote! { #field_ident: receivers.#field_ident, }
        })
        .collect();

    let outgoing_select_arms: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let rx_ident = format_ident!("{}_rx", p.name);
            let port_ident = format_ident!("{}", p.name);
            quote! {
                Some(value) = self.#rx_ident.recv() => {
                    self.ports.#port_ident.deliver(value);
                }
            }
        })
        .collect();

    let drain_ports_stmts: Vec<_> = ports_with_paths
        .iter()
        .filter_map(|(p, _, _)| match p {
            PortSpec::Outgoing(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let rx_ident = format_ident!("{}_rx", p.name);
            let port_ident = format_ident!("{}", p.name);
            quote! {
                while let Ok(v) = self.#rx_ident.try_recv() {
                    self.ports.#port_ident.deliver(v);
                }
            }
        })
        .collect();

    let receivers_unused_stmt = if has_outgoing_ports {
        quote! {}
    } else {
        quote! { let _ = &receivers; } // silence unused variable when no outgoing ports
    };

    let ports_dispatch_arm = if has_outgoing_ports {
        quote! {
            Request::Ports(req, _) => {
                match req {
                    #(#ports_match_arms)*
                }
            }
        }
    } else {
        quote! {}
    };

    let app_msg_from_bound = if has_ports {
        quote! { AppMsg: From<#msg_ty>, }
    } else {
        quote! {}
    };

    let ports_struct_field = if has_ports {
        quote! { ports: Ports<SelfMsg, AppMsg>, }
    } else {
        quote! {}
    };

    let ports_init_field = if has_ports {
        quote! { ports, }
    } else {
        quote! {}
    };

    let ports_method = if has_ports {
        quote! { pub fn ports(&self) -> Ports<SelfMsg, AppMsg> { self.ports.clone() } }
    } else {
        quote! {}
    };

    let ports_setup_stmts = if has_ports {
        quote! {
            let router = saucer_core::Router::new(app_tx.clone(), self_tx.clone());
            let (ports, receivers) = Ports::new(router.clone());
            #receivers_unused_stmt
        }
    } else {
        quote! {}
    };

    let reconciler_module = format_ident!("{}", reconciler_manager.module_name);
    let reconciler_type = format_ident!("{}", reconciler_manager.manager_type);
    let reconciler_variant = format_ident!("{}", reconciler_manager.variant);
    let reconciler_path = quote! { ::#reconciler_module::#reconciler_type };
    let (mapper_arg, mapper_body) = if reconciler_manager.self_msg_type == "()" {
        (quote! { _msg }, quote! { () })
    } else {
        (quote! { msg }, quote! { SelfMsg::#reconciler_variant(msg) })
    };

    quote! {
        type GeneratedAppMsg = #msg_ty;

        pub struct Runtime<Init, Update, ViewFn, Recon, Model, ViewOut, AppMsg = GeneratedAppMsg>
        where
            AppMsg: Clone + Send + 'static,
            #app_msg_from_bound
            SelfMsg: Clone + Send + 'static,
            Model: Send + 'static,
        {
            init: Option<Init>,
            update: Update,
            view: ViewFn,
            reconciler: Recon,
            observer: ObserverFn<AppMsg, Request<AppMsg>, SelfMsg>,
            #[allow(dead_code)] // ports-only runtimes store this even when no managers are present
            app_tx: tokio::sync::mpsc::UnboundedSender<AppMsg>,
            #ports_struct_field
            app_rx: tokio::sync::mpsc::UnboundedReceiver<AppMsg>,
            self_tx: tokio::sync::mpsc::UnboundedSender<SelfMsg>,
            #[allow(dead_code)]
            self_rx: tokio::sync::mpsc::UnboundedReceiver<SelfMsg>,
            req_tx: tokio::sync::mpsc::UnboundedSender<Request<AppMsg>>,
            req_rx: tokio::sync::mpsc::UnboundedReceiver<Request<AppMsg>>,
            #(#outgoing_receiver_fields_struct)*
            _model: std::marker::PhantomData<Model>,
            _view: std::marker::PhantomData<ViewOut>,
        }

        impl<Init, Update, ViewFn, Recon, Model, ViewOut, AppMsg> Runtime<Init, Update, ViewFn, Recon, Model, ViewOut, AppMsg>
        where
            Init: FnOnce() -> (Model, Cmd<AppMsg>),
            Update: Fn(Model, AppMsg) -> (Model, Cmd<AppMsg>),
            ViewFn: Fn(&Model) -> ViewOut,
            Recon: FnMut(&ViewOut, &saucer_core::SendToManager<#reconciler_path, SelfMsg>),
            Model: Send + 'static,
            SelfMsg: Clone + Send + 'static,
            AppMsg: Clone + Send + 'static,
            #app_msg_from_bound
        {
            fn drain_port_queues(&mut self) {
                #(#drain_ports_stmts)*
            }

            pub fn new(
                init: Init,
                update: Update,
                view: ViewFn,
                reconciler: Recon,
                observer: ObserverFn<AppMsg, Request<AppMsg>, SelfMsg>,
            ) -> Self {
                let (app_tx, app_rx) = tokio::sync::mpsc::unbounded_channel();
                let (self_tx, self_rx) = tokio::sync::mpsc::unbounded_channel();
                let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
                #ports_setup_stmts

                Self {
                    init: Some(init),
                    update,
                    view,
                    reconciler,
                    observer,
                    app_tx,
                    #ports_init_field
                    app_rx,
                    self_tx,
                    self_rx,
                    req_tx,
                    req_rx,
                    #(#outgoing_receiver_init_fields)*
                    _model: std::marker::PhantomData,
                    _view: std::marker::PhantomData,
                }
            }

            #ports_method

            fn enqueue_cmd(
                tx: &tokio::sync::mpsc::UnboundedSender<Request<AppMsg>>,
                cmd: Cmd<AppMsg>,
            ) {
                for req in cmd.into_inner() {
                    let _ = tx.send(req);
                }
            }

            pub async fn run(mut self) {
                let (mut model, init_cmd) = self
                    .init
                    .take()
                        .expect("Runtime::run called more than once")();

                let observer = self.observer.clone();
                Self::enqueue_cmd(&self.req_tx, init_cmd);

                #(#router_decls)*
                #(#manager_init)*

                let mut view_cache = (self.view)(&model);
                let sender = saucer_core::SendToManager::<#reconciler_path, SelfMsg>::new(
                    self.self_tx.clone(),
                    |#mapper_arg| { #mapper_body },
                );
                (self.reconciler)(&view_cache, &sender);

                loop {
                    tokio::select! {
                        Some(req) = self.req_rx.recv() => {
                            let observation = Observation::Effect { ts: std::time::SystemTime::now(), data: req.clone() };
                            observer(&observation);
                            match req {
                                Request::Core(saucer_core::CoreRequest::Shutdown) => { self.drain_port_queues(); break; },
                                #(#request_dispatch)*
                                #ports_dispatch_arm
                            }
                        }
                        Some(app_evt) = self.app_rx.recv() => {
                            let observation = Observation::Event { ts: std::time::SystemTime::now(), data: app_evt.clone() };
                            observer(&observation);
                            let (new_model, cmd) = (self.update)(model, app_evt);
                            model = new_model;
                            Self::enqueue_cmd(&self.req_tx, cmd);
                            view_cache = (self.view)(&model);
                            (self.reconciler)(&view_cache, &sender);
                        }
                        #self_msg_dispatch_arm
                        #(#outgoing_select_arms)*
                    }
                }
            }
        }
    }
}

fn generate_template_module_with_source(
    template: &TemplateInfo,
    transformed_source: String,
    outgoing_ports: &[PortSpec],
    _managers: &[ManagerInfo],
) -> TokenStream {
    let module_ident = format_ident!("{}", template.module_name);

    let parsed: syn::File = syn::parse_str(&transformed_source).unwrap_or_else(|e| {
        panic!(
            "Failed to parse transformed template: {}\n\nSource:\n{}",
            e, transformed_source
        )
    });

    let items = parsed.items;

    let needs_cmd_ext = transformed_source.contains(".map(");
    let mut prelude_items: Vec<TokenStream> = Vec::new();
    if needs_cmd_ext {
        prelude_items.push(quote! { #[allow(unused_imports)] use super::super::CmdExt; });
    }
    if !outgoing_ports.is_empty() {
        let names: Vec<_> = outgoing_ports
            .iter()
            .filter_map(|p| match p {
                PortSpec::Outgoing(p) => Some(format_ident!("{}", p.name)),
                _ => None,
            })
            .collect();
        prelude_items
            .push(quote! { #[allow(unused_imports)] use super::super::{ #( #names ),* }; });
    }
    let prelude = quote! { #(#prelude_items)* };

    quote! {
        pub mod #module_ident {
            #prelude
            #(#items)*
        }
    }
}
