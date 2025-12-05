use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::build::ast::{ManagerInfo, PortSpec};
use crate::build::ports;

/// Generate the Request enum (and PortsRequest) plus map() helper and redacted Debug.
pub fn generate_request_enum(managers: &[ManagerInfo], ports: &[PortSpec]) -> TokenStream {
    let variants: Vec<_> = managers
        .iter()
        .map(|m| {
            let variant_ident = format_ident!("{}", m.variant);
            let module_ident = format_ident!("{}", m.module_name);
            let request_ident = format_ident!("{}", m.request_type);
            quote! {
                #variant_ident(::#module_ident::#request_ident<Msg>)
            }
        })
        .collect();

    let variant_idents: Vec<_> = managers
        .iter()
        .map(|m| format_ident!("{}", m.variant))
        .collect();

    let ports_variant = if ports.iter().any(|p| matches!(p, PortSpec::Outgoing(_))) {
        Some(quote! { Ports(PortsRequest, std::marker::PhantomData<Msg>) })
    } else {
        None
    };

    let mut request_variants: Vec<TokenStream> = Vec::new();
    request_variants.push(quote! { Core(saucer_core::CoreRequest) });
    request_variants.extend(variants.into_iter());
    if let Some(p) = &ports_variant {
        request_variants.push(p.clone());
    }

    let ports_variant_idents: Vec<_> = ports
        .iter()
        .filter_map(|p| match p {
            PortSpec::Outgoing(p) => Some(format_ident!("{}", ports::to_camel(&p.name))),
            _ => None,
        })
        .collect();

    let map_arms: Vec<_> = managers
        .iter()
        .map(|m| {
            let variant_ident = format_ident!("{}", m.variant);
            quote! { Request::#variant_ident(req) => Request::#variant_ident(req.map(f)) }
        })
        .collect();

    let mut map_match_arms: Vec<TokenStream> =
        vec![quote! { Request::Core(core) => Request::Core(core) }];
    map_match_arms.extend(map_arms);
    if ports_variant.is_some() {
        map_match_arms.push(quote! {
            Request::Ports(p, _) => Request::Ports(p, std::marker::PhantomData)
        });
    }

    let map_impl = quote! {
        #[allow(dead_code)]
        impl<Msg: 'static> Request<Msg> {
            #[allow(dead_code)]
            fn map<Msg2>(self, f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static) -> Request<Msg2> {
                let _ = &f;
                match self {
                    #(#map_match_arms),*
                }
            }
        }
    };

    let ports_request_enum = if ports_variant.is_some() {
        let variants: Vec<_> = ports
            .iter()
            .filter_map(|p| match p {
                PortSpec::Outgoing(p) => Some(p),
                _ => None,
            })
            .map(|p| {
                let v = format_ident!("{}", ports::to_camel(&p.name));
                let ty: TokenStream = ports::payload_type_tokens(&PortSpec::Outgoing(p.clone()));
                quote! { #v { value: #ty } }
            })
            .collect();
        quote! {
            #[derive(Clone)]
            pub enum PortsRequest {
                #(#variants),*
            }
        }
    } else {
        quote! {}
    };

    let ports_request_debug = if ports_variant.is_some() {
        quote! {
            impl std::fmt::Debug for PortsRequest {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        #(PortsRequest::#ports_variant_idents { .. } => f.write_str(concat!("PortsRequest::", stringify!(#ports_variant_idents))),)*
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    let match_arms: Vec<_> = variant_idents
        .iter()
        .map(|ident| {
            quote! {
                Request::#ident(req) => f.debug_tuple(concat!("Request::", stringify!(#ident)))
                    .field(req)
                    .finish(),
            }
        })
        .collect();

    let ports_debug_arm = if ports_variant.is_some() {
        quote! { Request::Ports(p, _) => f.debug_tuple("Request::Ports").field(p).finish(), }
    } else {
        quote! {}
    };

    quote! {
        #ports_request_enum
        #[allow(dead_code)]
        #[derive(Clone)]
        pub enum Request<Msg> {
            #(#request_variants),*
        }

        impl<Msg> std::fmt::Debug for Request<Msg> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Request::Core(_) => f.write_str("Request::Core"),
                    #(#match_arms)*
                    #ports_debug_arm
                }
            }
        }

        #ports_request_debug

        #map_impl
    }
}

/// Generate the unified SelfMsg type (enum over managers with self-msgs).
pub fn generate_self_msg_type(managers: &[ManagerInfo]) -> TokenStream {
    let with_self_msgs: Vec<_> = managers
        .iter()
        .filter(|m| m.self_msg_type != "()")
        .collect();

    if with_self_msgs.is_empty() {
        quote! { pub type SelfMsg = (); }
    } else {
        let variants: Vec<_> = with_self_msgs
            .iter()
            .map(|m| {
                let variant_ident = format_ident!("{}", m.variant);
                let module_ident = format_ident!("{}", m.module_name);
                let ty_ident = format_ident!("{}", m.self_msg_type);
                quote! { #variant_ident(::#module_ident::#ty_ident) }
            })
            .collect();

        quote! {
            #[allow(dead_code)]
            #[derive(Clone, Debug)]
            pub enum SelfMsg {
                #(#variants),*
            }
        }
    }
}
