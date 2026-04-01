use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Field, Fields, GenericParam, ItemStruct, Type, parse_macro_input,
    parse_quote,
};

#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !proc_macro2::TokenStream::from(attr).is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "seekwel::model does not take arguments",
        )
        .to_compile_error()
        .into();
    }

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

    item.attrs.push(parse_quote!(#[derive(seekwel::Model)]));

    quote!(#item).into()
}

#[proc_macro_derive(Model)]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let vis = &input.vis;
    let table_name = name.to_string().to_lowercase();
    let builder_name = format_ident!("{}Builder", name);

    if let Err(error) = validate_typestate_generics(&input.generics) {
        return error.to_compile_error().into();
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input,
                    "Model only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "Model can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let id_fields: Vec<_> = fields
        .iter()
        .filter(|field| field.ident.as_ref().is_some_and(|ident| ident == "id"))
        .collect();
    if id_fields.len() != 1 {
        return syn::Error::new_spanned(name, "Model structs must contain exactly one `id` field")
            .to_compile_error()
            .into();
    }
    if !is_u64_type(&id_fields[0].ty) {
        return syn::Error::new_spanned(&id_fields[0].ty, "The `id` field must have type `u64`")
            .to_compile_error()
            .into();
    }

    let state_fields: Vec<_> = fields
        .iter()
        .filter(|field| is_phantom_data_type(&field.ty))
        .collect();
    if state_fields.len() != 1 {
        return syn::Error::new_spanned(
            name,
            "Model structs must contain exactly one PhantomData typestate field; use #[seekwel::model] to have it injected automatically",
        )
        .to_compile_error()
        .into();
    }
    let state_field_name = state_fields[0].ident.as_ref().unwrap();

    let columns: Vec<_> = fields
        .iter()
        .filter(|field| {
            let ident = field.ident.as_ref().unwrap();
            ident != "id" && !is_phantom_data_type(&field.ty)
        })
        .collect();

    let col_names: Vec<_> = columns
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect();
    let col_types: Vec<_> = columns.iter().map(|field| &field.ty).collect();

    let column_defs = columns.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let (sql_type, nullable) = sql_type_for(&field.ty);
        quote! {
            seekwel::model::ColumnDef {
                name: #field_name,
                sql_type: #sql_type,
                nullable: #nullable,
            }
        }
    });

    let from_row_fields = columns.iter().enumerate().map(|(index, field)| {
        let field_name = field.ident.as_ref().unwrap();
        let row_index = index + 1;
        if needs_i64_cast(&field.ty) {
            quote! { #field_name: row.get::<_, i64>(#row_index)? as u64 }
        } else {
            quote! { #field_name: row.get(#row_index)? }
        }
    });

    let param_exprs = columns.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        to_value_expr(&field.ty, quote! { self.#field_name })
    });

    let builder_fields = col_names
        .iter()
        .zip(col_types.iter())
        .map(|(field_name, ty)| {
            quote! { #field_name: Option<#ty> }
        });

    let builder_defaults = col_names.iter().map(|field_name| {
        quote! { #field_name: None }
    });

    let builder_setters = columns.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        if is_option_type(ty) {
            quote! {
                pub fn #field_name(mut self, value: #ty) -> Self {
                    self.#field_name = Some(value);
                    self
                }
            }
        } else {
            quote! {
                pub fn #field_name(mut self, value: impl Into<#ty>) -> Self {
                    self.#field_name = Some(value.into());
                    self
                }
            }
        }
    });

    let build_extracts = columns.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let missing_name = field_name.to_string();
        if is_option_type(&field.ty) {
            quote! { let #field_name = self.#field_name.unwrap_or(None); }
        } else {
            quote! {
                let #field_name = self
                    .#field_name
                    .ok_or_else(|| seekwel::error::Error::MissingField(#missing_name.to_string()))?;
            }
        }
    });

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics seekwel::model::Model for #name #ty_generics #where_clause {
            fn table_name() -> &'static str {
                #table_name
            }

            fn columns() -> &'static [seekwel::model::ColumnDef] {
                &[#(#column_defs,)*]
            }

            fn params(&self) -> Vec<rusqlite::types::Value> {
                vec![#(#param_exprs,)*]
            }
        }

        impl seekwel::model::PersistedModel for #name<seekwel::Persisted> {
            fn id(&self) -> u64 {
                self.id
            }

            fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
                Ok(Self {
                    id: row.get::<_, i64>(0)? as u64,
                    #(#from_row_fields,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }
        }

        impl #name<seekwel::Persisted> {
            pub fn builder() -> #builder_name {
                #builder_name {
                    #(#builder_defaults,)*
                }
            }

            pub fn create_table() -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::Model>::create_table()
            }

            pub fn reload(&mut self) -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::PersistedModel>::reload(self)
            }
        }

        impl #name<seekwel::NewRecord> {
            pub fn save(self) -> Result<#name<seekwel::Persisted>, seekwel::error::Error> {
                let id = seekwel::model::insert(&self)?;
                Ok(#name {
                    id,
                    #(#col_names: self.#col_names,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }
        }

        #vis struct #builder_name {
            #(#builder_fields,)*
        }

        impl #builder_name {
            #(#builder_setters)*

            pub fn build(self) -> Result<#name<seekwel::NewRecord>, seekwel::error::Error> {
                #(#build_extracts)*

                Ok(#name {
                    id: 0,
                    #(#col_names,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }

            pub fn create(self) -> Result<#name<seekwel::Persisted>, seekwel::error::Error> {
                self.build()?.save()
            }
        }
    };

    TokenStream::from(expanded)
}

fn validate_typestate_generics(generics: &syn::Generics) -> Result<(), syn::Error> {
    if generics.params.len() != 1 {
        return Err(syn::Error::new_spanned(
            generics,
            "Model derives require exactly one typestate type parameter; use #[seekwel::model] to generate it automatically",
        ));
    }

    match generics.params.first() {
        Some(GenericParam::Type(_)) => Ok(()),
        _ => Err(syn::Error::new_spanned(
            generics,
            "Model derives only support a single typestate type parameter",
        )),
    }
}

/// Returns (sql_type, nullable)
fn sql_type_for(ty: &Type) -> (&'static str, bool) {
    if let Some(inner) = option_inner_type(ty) {
        let (sql_type, _) = sql_type_for(inner);
        return (sql_type, true);
    }

    let type_str = quote!(#ty).to_string();
    match type_str.as_str() {
        "String" => ("TEXT", false),
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "bool" => ("INTEGER", false),
        "f32" | "f64" => ("REAL", false),
        _ => ("TEXT", false),
    }
}

fn is_u64_type(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string();
    type_str == "u64"
}

fn is_phantom_data_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        return type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "PhantomData");
    }

    false
}

/// Generate a `rusqlite::types::Value` expression for a field.
fn to_value_expr(ty: &Type, field: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    if let Some(inner) = option_inner_type(ty) {
        let inner_expr = to_value_expr(inner, quote! { v });
        return quote! {
            match #field {
                Some(v) => #inner_expr,
                None => rusqlite::types::Value::Null,
            }
        };
    }

    let type_str = quote!(#ty).to_string();
    match type_str.as_str() {
        "String" => quote! { rusqlite::types::Value::Text(#field.clone()) },
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => {
            quote! { rusqlite::types::Value::Integer(#field as i64) }
        }
        "bool" => quote! { rusqlite::types::Value::Integer(#field as i64) },
        "f32" | "f64" => quote! { rusqlite::types::Value::Real(#field as f64) },
        _ => quote! { rusqlite::types::Value::Text(#field.to_string()) },
    }
}

fn needs_i64_cast(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string();
    type_str == "u64"
}

fn is_option_type(ty: &Type) -> bool {
    option_inner_type(ty).is_some()
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Option"
            && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return Some(inner);
        }
    }
    None
}
