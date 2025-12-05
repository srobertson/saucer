use crate::build::ast::{IncomingPort, OutgoingPort, PortDirection, PortSpec};
use quote::ToTokens;
use syn::{FnArg, Item, Pat, ReturnType};

/// Parse ports declared in a template source file.
pub fn parse_ports(source: &str) -> Vec<PortSpec> {
    let file = syn::parse_file(source).expect("Failed to parse template for ports");
    let mut ports = Vec::new();
    for item in file.items {
        if let Item::Fn(func) = item {
            let has_port_attr = func.attrs.iter().any(|a| a.path().is_ident("port"));
            if !has_port_attr {
                continue;
            }
            let name = func.sig.ident.to_string();
            let ret_ty = match &func.sig.output {
                ReturnType::Type(_, ty) => ty.as_ref(),
                ReturnType::Default => panic!("Port function `{}` must have a return type", name),
            };
            let direction = match ret_ty {
                syn::Type::Path(tp) => {
                    let last = tp
                        .path
                        .segments
                        .last()
                        .map(|seg| seg.ident.to_string())
                        .unwrap_or_default();
                    match last.as_str() {
                        "Cmd" => PortDirection::Outgoing,
                        "Sub" => PortDirection::Incoming,
                        _ => panic!("Port function `{}` must return Sub<Msg> (incoming) or Cmd<Msg> (outgoing)", name),
                    }
                }
                _ => panic!("Port function `{}` must return Sub<Msg> or Cmd<Msg>", name),
            };

            let mut args = Vec::new();
            for arg in func.sig.inputs.iter() {
                match arg {
                    FnArg::Typed(pt) => {
                        let ident = match &*pt.pat {
                            Pat::Ident(id) => id.ident.to_string(),
                            _ => {
                                panic!("Port function `{}` uses unsupported pattern argument", name)
                            }
                        };
                        let ty = pt.ty.to_token_stream().to_string();
                        args.push((ident, ty));
                    }
                    FnArg::Receiver(_) => {
                        panic!("Port function `{}` must be free, not a method", name);
                    }
                }
            }

            let spec = match direction {
                PortDirection::Incoming => PortSpec::Incoming(IncomingPort { name, args }),
                PortDirection::Outgoing => PortSpec::Outgoing(OutgoingPort { name, args }),
            };
            ports.push(spec);
        }
    }
    ports
}

pub fn payload_type_tokens(port: &PortSpec) -> proc_macro2::TokenStream {
    let args: &Vec<(String, String)> = match port {
        PortSpec::Incoming(p) => &p.args,
        PortSpec::Outgoing(p) => &p.args,
    };
    let ty: String = match args.len() {
        0 => "()".to_string(),
        1 => args[0].1.clone(),
        _ => format!(
            "({})",
            args.iter()
                .map(|(_, t)| t.clone())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    ty.parse().expect("payload type should parse")
}

pub(crate) fn to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in s.chars() {
        if c == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}
