use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields};

#[proc_macro_derive(Request)]
pub fn derive_request(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let body = match input.data {
        Data::Struct(ref data) => {
            struct_debug(&name, (impl_generics, ty_generics, where_clause), data)
        }
        Data::Enum(ref data) => enum_debug(&name, (impl_generics, ty_generics, where_clause), data),
        Data::Union(_) => {
            return syn::Error::new_spanned(name, "Request derive does not support unions")
                .to_compile_error()
                .into();
        }
    };

    body.into()
}

fn is_redacted(field: &syn::Field) -> bool {
    field
        .ident
        .as_ref()
        .map(|id| id == "returns" || id == "tools")
        .unwrap_or(false)
}

fn struct_debug(
    name: &syn::Ident,
    generics: (
        syn::ImplGenerics<'_>,
        syn::TypeGenerics<'_>,
        Option<&syn::WhereClause>,
    ),
    data: &syn::DataStruct,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics;
    match &data.fields {
        Fields::Named(fields) => {
            let writers: Vec<_> = fields
                .named
                .iter()
                .filter_map(|f| {
                    let ident = f.ident.as_ref().unwrap();
                    if is_redacted(f) {
                        None
                    } else {
                        Some(quote! { .field(stringify!(#ident), &self.#ident) })
                    }
                })
                .collect();
            quote! {
                impl #impl_generics std::fmt::Debug for #name #ty_generics #where_clause {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.debug_struct(stringify!(#name))
                            #(#writers)*
                            .finish()
                    }
                }
            }
        }
        Fields::Unnamed(fields) => {
            let elems: Vec<_> = fields
                .unnamed
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    let idx = syn::Index::from(i);
                    if is_redacted(f) {
                        None
                    } else {
                        Some(quote! { &self.#idx })
                    }
                })
                .collect();
            quote! {
                impl #impl_generics std::fmt::Debug for #name #ty_generics #where_clause {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        let mut d = f.debug_tuple(stringify!(#name));
                        #( d.field(#elems); )*
                        d.finish()
                    }
                }
            }
        }
        Fields::Unit => {
            quote! {
                impl #impl_generics std::fmt::Debug for #name #ty_generics #where_clause {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str(stringify!(#name))
                    }
                }
            }
        }
    }
}

fn enum_debug(
    name: &syn::Ident,
    generics: (
        syn::ImplGenerics<'_>,
        syn::TypeGenerics<'_>,
        Option<&syn::WhereClause>,
    ),
    data: &syn::DataEnum,
) -> proc_macro2::TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics;
    let arms: Vec<_> = data.variants.iter().map(|v| {
        let vident = &v.ident;
        match &v.fields {
            Fields::Unit => {
                quote! { #name::#vident => f.write_str(concat!(stringify!(#name), "::", stringify!(#vident))) }
            }
            Fields::Unnamed(fields) => {
                let bindings: Vec<_> = (0..fields.unnamed.len()).map(|i| format_ident!("f{}", i)).collect();
                let writes: Vec<_> = fields.unnamed.iter().enumerate().filter_map(|(i, fld)| {
                    let b = &bindings[i];
                    if is_redacted(fld) {
                        None
                    } else {
                        Some(quote! { dbg_fields.field(#b); })
                    }
                }).collect();
                quote! {
                    #name::#vident( #( ref #bindings ),* ) => {
                        let mut dbg_fields = f.debug_tuple(stringify!(#vident));
                        #(#writes)*
                        dbg_fields.finish()
                    }
                }
            }
            Fields::Named(fields) => {
                let bindings: Vec<_> = fields.named.iter().map(|fld| fld.ident.clone().unwrap()).collect();
                let writes: Vec<_> = fields.named.iter().filter_map(|fld| {
                    let id = fld.ident.as_ref().unwrap();
                    if is_redacted(fld) {
                        None
                    } else {
                        Some(quote! { dbg_fields.field(stringify!(#id), #id); })
                    }
                }).collect();
                quote! {
                    #name::#vident { #( ref #bindings ),* } => {
                        let mut dbg_fields = f.debug_struct(stringify!(#vident));
                        #(#writes)*
                        dbg_fields.finish()
                    }
                }
            }
        }
    }).collect();

    quote! {
        impl #impl_generics std::fmt::Debug for #name #ty_generics #where_clause {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    #(#arms),*
                }
            }
        }
    }
}
