mod analysis;
mod codegen;
mod ir;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Field, Fields, ItemStruct, LitBool, LitStr, Token, parse_macro_input, parse_quote};

struct ModelArgs {
    table_name: Option<LitStr>,
    primary_key: Option<LitStr>,
    auto_increment: Option<LitBool>,
}

impl Parse for ModelArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self {
            table_name: None,
            primary_key: None,
            auto_increment: None,
        };

        while !input.is_empty() {
            let key = input.parse::<syn::Ident>()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "table_name" => {
                    if args.table_name.is_some() {
                        return Err(syn::Error::new_spanned(key, "duplicate `table_name` option"));
                    }
                    args.table_name = Some(input.parse()?);
                }
                "primary_key" => {
                    if args.primary_key.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "duplicate `primary_key` option",
                        ));
                    }
                    args.primary_key = Some(input.parse()?);
                }
                "auto_increment" => {
                    if args.auto_increment.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "duplicate `auto_increment` option",
                        ));
                    }
                    args.auto_increment = Some(input.parse()?);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        key,
                        "unsupported seekwel::model option; expected `table_name`, `primary_key`, or `auto_increment`",
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(args)
    }
}

pub(crate) fn expand_model_attribute(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ModelArgs);
    let mut item = parse_macro_input!(item as ItemStruct);

    if !item.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &item.generics,
            "seekwel::model only supports non-generic structs; the macro injects the typestate generic for you",
        )
        .to_compile_error()
        .into();
    }

    let Fields::Named(fields) = &mut item.fields else {
        return syn::Error::new_spanned(
            &item,
            "seekwel::model only supports structs with named fields",
        )
        .to_compile_error()
        .into();
    };

    if fields.named.iter().any(|field| {
        field
            .ident
            .as_ref()
            .is_some_and(|ident| ident == "__seekwel_state")
    }) {
        return syn::Error::new_spanned(
            &item,
            "field name __seekwel_state is reserved by seekwel::model",
        )
        .to_compile_error()
        .into();
    }

    item.generics = parse_quote!(<S = seekwel::Persisted>);

    let state_field: Field = parse_quote! {
        __seekwel_state: std::marker::PhantomData<S>
    };
    fields.named.push(state_field);

    item.attrs
        .push(parse_quote!(#[derive(seekwel::Model, Clone)]));

    let mut seekwel_options = Vec::<proc_macro2::TokenStream>::new();
    if let Some(table_name) = args.table_name {
        seekwel_options.push(quote!(table_name = #table_name));
    }
    if let Some(primary_key) = args.primary_key {
        seekwel_options.push(quote!(primary_key = #primary_key));
    }
    if let Some(auto_increment) = args.auto_increment {
        seekwel_options.push(quote!(auto_increment = #auto_increment));
    }
    if !seekwel_options.is_empty() {
        item.attrs.push(parse_quote!(#[seekwel(#(#seekwel_options),*)]));
    }

    quote!(#item).into()
}

pub(crate) fn expand_model_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);

    match analysis::analyze_model(input) {
        Ok(spec) => codegen::expand_model(&spec).into(),
        Err(error) => error.to_compile_error().into(),
    }
}
