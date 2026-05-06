use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::input::{FieldInput, FieldKind, RecordInput};

pub fn expand(input: &RecordInput) -> TokenStream {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let krate = input.crate_tokens();

    let from_fields = input.fields.iter().map(|f| from_field(f, &krate));
    let to_fields = input.fields.iter().map(|f| to_field(f, &krate));

    let record_const = input.record_type.as_ref().map(|s| {
        let s = s.as_str();
        quote! { pub const RECORD: &'static str = #s; }
    });

    quote! {
        #[automatically_derived]
        impl #impl_generics #ident #ty_generics #where_clause {
            #record_const

            /// Build a value from a parameter map. Missing or unparseable fields
            /// fall back to `Default::default()`.
            pub fn from_params(
                params: &#krate::parameter::ParameterMap,
            ) -> Self {
                Self {
                    #(#from_fields,)*
                }
            }

            /// Write the value's fields into a parameter map. Default-valued
            /// fields are skipped, matching Altium's omit-when-zero convention.
            pub fn to_params(
                &self,
                params: &mut #krate::parameter::ParameterMap,
            ) {
                #(#to_fields)*
            }
        }
    }
}

fn from_field(field: &FieldInput, krate: &TokenStream) -> TokenStream {
    let ident = &field.ident;
    let key = &field.param_name;
    if field.skip {
        return quote! { #ident: ::core::default::Default::default() };
    }
    match field.kind {
        FieldKind::Bool => quote! { #ident: params.get_bool(#key) },
        FieldKind::SignedInt => {
            let ty = &field.ty;
            quote! { #ident: params.get(#key).and_then(|s| s.trim().parse::<#ty>().ok()).unwrap_or_default() }
        }
        FieldKind::UnsignedInt => {
            let ty = &field.ty;
            quote! { #ident: params.get(#key).and_then(|s| s.trim().parse::<#ty>().ok()).unwrap_or_default() }
        }
        FieldKind::Float => {
            let ty = &field.ty;
            quote! { #ident: params.get(#key).and_then(|s| s.trim().parse::<#ty>().ok()).unwrap_or_default() }
        }
        FieldKind::String => {
            quote! { #ident: params.get(#key).map(::core::convert::From::from).unwrap_or_default() }
        }
        FieldKind::OptionString => {
            quote! { #ident: params.get(#key).map(::core::convert::From::from) }
        }
        FieldKind::Coord => {
            quote! {
                #ident: params
                    .get(#key)
                    .and_then(|s| s.parse::<#krate::Coord>().ok())
                    .unwrap_or(#krate::Coord::ZERO)
            }
        }
        FieldKind::Enum => {
            let ty = &field.ty;
            quote! {
                #ident: params
                    .get(#key)
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .and_then(|v| <#ty as ::core::convert::TryFrom<i32>>::try_from(v).ok())
                    .unwrap_or_default()
            }
        }
    }
}

fn to_field(field: &FieldInput, krate: &TokenStream) -> TokenStream {
    if field.skip {
        return quote! {};
    }
    let ident = &field.ident;
    let key = &field.param_name;
    let scratch = format_ident!("__altium_value_{}", ident);
    match field.kind {
        FieldKind::Bool => quote! {
            if self.#ident {
                params.insert(#key, "T");
            }
        },
        FieldKind::SignedInt | FieldKind::UnsignedInt => quote! {
            if self.#ident != 0 {
                params.insert(#key, ::std::string::ToString::to_string(&self.#ident));
            }
        },
        FieldKind::Float => quote! {
            if self.#ident != 0.0 {
                params.insert(#key, ::std::string::ToString::to_string(&self.#ident));
            }
        },
        FieldKind::String => quote! {
            if !self.#ident.is_empty() {
                params.insert(#key, self.#ident.clone());
            }
        },
        FieldKind::OptionString => quote! {
            if let ::core::option::Option::Some(ref #scratch) = self.#ident {
                if !#scratch.is_empty() {
                    params.insert(#key, #scratch.clone());
                }
            }
        },
        FieldKind::Coord => quote! {
            if self.#ident != #krate::Coord::ZERO {
                params.insert(#key, ::std::string::ToString::to_string(&self.#ident));
            }
        },
        FieldKind::Enum => quote! {
            {
                let #scratch: i32 = ::core::convert::Into::into(self.#ident);
                if #scratch != 0 {
                    params.insert(#key, ::std::string::ToString::to_string(&#scratch));
                }
            }
        },
    }
}
