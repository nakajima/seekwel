use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Field, Fields, GenericParam, Ident, ItemStruct, Type, parse_macro_input,
    parse_quote,
};

struct ModelField<'a> {
    ident: &'a Ident,
    ty: &'a Type,
    field_name: String,
    storage_column_name: String,
    query_variant: Ident,
    is_optional: bool,
    optional_inner_ty: Option<&'a Type>,
    relation_target: Option<&'a Type>,
}

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

    item.attrs
        .push(parse_quote!(#[derive(seekwel::Model, Clone)]));

    quote!(#item).into()
}

#[proc_macro_derive(Model)]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let vis = &input.vis;
    let table_name = name.to_string().to_lowercase();
    let builder_name = format_ident!("{}Builder", name);
    let columns_name = format_ident!("{}Columns", name);

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

    let columns: Vec<_> = match fields
        .iter()
        .filter(|field| {
            let ident = field.ident.as_ref().unwrap();
            ident != "id" && !is_phantom_data_type(&field.ty)
        })
        .map(analyze_model_field)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(columns) => columns,
        Err(error) => return error.to_compile_error().into(),
    };

    let col_names: Vec<_> = columns.iter().map(|field| field.ident).collect();
    let column_variants: Vec<_> = columns.iter().map(|field| &field.query_variant).collect();
    let column_names: Vec<_> = columns
        .iter()
        .map(|field| field.storage_column_name.as_str())
        .collect();
    let column_variant_docs = column_names.iter().map(|column_name| {
        quote! {
            #[doc = concat!("The `", #column_name, "` column.")]
        }
    });
    let column_defs = columns.iter().map(|field| {
        let column_name = &field.storage_column_name;
        let ty = field.ty;
        quote! {
            seekwel::model::column::<#ty>(#column_name)
        }
    });

    let from_row_fields = columns.iter().enumerate().map(|(index, field)| {
        let field_name = field.ident;
        let ty = field.ty;
        let row_index = index + 1;
        quote! {
            #field_name: <#ty as seekwel::model::SqlField>::from_sql_row(row, #row_index)?
        }
    });

    let param_exprs = columns.iter().map(|field| {
        let field_name = field.ident;
        let ty = field.ty;
        quote! {
            <#ty as seekwel::model::SqlField>::to_sql_value(&self.#field_name)
        }
    });

    let builder_fields = columns.iter().map(|field| {
        let field_name = field.ident;
        if let Some(inner_ty) = field.optional_inner_ty {
            quote! { #field_name: seekwel::model::builder::Optional<#inner_ty> }
        } else {
            let ty = field.ty;
            quote! { #field_name: seekwel::model::builder::Required<#ty> }
        }
    });

    let builder_defaults = col_names.iter().map(|field_name| {
        quote! { #field_name: Default::default() }
    });

    let builder_setters = columns.iter().map(|field| {
        let field_name = field.ident;
        let field_name_str = field.field_name.as_str();
        if field.is_optional {
            if let Some(relation_ty) = field
                .optional_inner_ty
                .filter(|_| field.relation_target.is_some())
            {
                quote! {
                    #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                    pub fn #field_name<V>(mut self, value: Option<V>) -> Self
                    where
                        V: Into<#relation_ty>,
                    {
                        self.#field_name.set(value.map(Into::into));
                        self
                    }
                }
            } else {
                let ty = field.ty;
                quote! {
                    #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                    pub fn #field_name(mut self, value: #ty) -> Self {
                        self.#field_name.set(value);
                        self
                    }
                }
            }
        } else {
            let ty = field.ty;
            quote! {
                #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                pub fn #field_name(mut self, value: impl Into<#ty>) -> Self {
                    self.#field_name.set(value);
                    self
                }
            }
        }
    });

    let build_extracts = columns.iter().map(|field| {
        let field_name = field.ident;
        let missing_name = field.field_name.as_str();
        if field.is_optional {
            quote! { let #field_name = self.#field_name.finish(); }
        } else {
            quote! {
                let #field_name = self.#field_name.finish(#missing_name)?;
            }
        }
    });

    let relation_methods = columns.iter().filter_map(|field| {
        let relation_target = field.relation_target?;
        let field_name = field.ident;
        let field_name_str = field.field_name.as_str();

        Some(if field.is_optional {
            quote! {
                #[doc = concat!("Loads the `", #field_name_str, "` relation.")]
                pub fn #field_name(&self) -> Result<Option<#relation_target>, seekwel::error::Error>
                where
                    #relation_target: seekwel::model::PersistedModel + Clone,
                {
                    self.#field_name
                        .as_ref()
                        .map(|relation| relation.load())
                        .transpose()
                }
            }
        } else {
            quote! {
                #[doc = concat!("Loads the `", #field_name_str, "` relation.")]
                pub fn #field_name(&self) -> Result<#relation_target, seekwel::error::Error>
                where
                    #relation_target: seekwel::model::PersistedModel + Clone,
                {
                    self.#field_name.load()
                }
            }
        })
    });

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        #[doc = concat!("Typed columns for [`", stringify!(#name), "`].")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #vis enum #columns_name {
            #[doc = "The `id` column."]
            Id,
            #(#column_variant_docs #column_variants,)*
        }

        impl #columns_name {
            #[doc = "Returns an ascending `ORDER BY` clause for this column."]
            pub fn asc(self) -> seekwel::Order {
                seekwel::Order::asc(self)
            }

            #[doc = "Returns a descending `ORDER BY` clause for this column."]
            pub fn desc(self) -> seekwel::Order {
                seekwel::Order::desc(self)
            }
        }

        impl seekwel::model::Column for #columns_name {
            fn as_str(self) -> &'static str {
                match self {
                    Self::Id => "id",
                    #(Self::#column_variants => #column_names,)*
                }
            }
        }

        impl #impl_generics seekwel::model::Model for #name #ty_generics #where_clause {
            type Column = #columns_name;

            fn table_name() -> &'static str {
                #table_name
            }

            fn columns() -> &'static [seekwel::model::ColumnDef] {
                const COLUMNS: &[seekwel::model::ColumnDef] = &[#(#column_defs,)*];
                COLUMNS
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
                    id: <u64 as seekwel::model::SqlField>::from_sql_row(row, 0)?,
                    #(#from_row_fields,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }
        }

        impl #name<seekwel::Persisted> {
            #[doc = concat!("Creates a builder for [`", stringify!(#name), "<seekwel::NewRecord>`].")]
            pub fn builder() -> #builder_name {
                #builder_name {
                    #(#builder_defaults,)*
                }
            }

            #[doc = "Creates the backing SQLite table if it does not already exist."]
            pub fn create_table() -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::Model>::create_table()
            }

            #[doc = "Finds a persisted record by primary key."]
            pub fn find(id: u64) -> Result<Self, seekwel::error::Error> {
                <Self as seekwel::model::PersistedModel>::find(id)
            }

            #[doc = "Starts a typed query for this model."]
            pub fn q(
                column: #columns_name,
                comparison: seekwel::model::Comparison,
            ) -> seekwel::model::Query<Self> {
                seekwel::model::Query::new(column, comparison)
            }

            #[doc = "Persists the current in-memory field values back to the database."]
            pub fn save(&self) -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::PersistedModel>::save(self)
            }

            #[doc = "Reloads this persisted record from the database."]
            pub fn reload(&mut self) -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::PersistedModel>::reload(self)
            }

            #[doc = "Deletes this persisted record from the database."]
            pub fn delete(self) -> Result<(), seekwel::error::Error> {
                <Self as seekwel::model::PersistedModel>::delete(self)
            }

            #(#relation_methods)*
        }

        impl #name<seekwel::NewRecord> {
            #[doc = "Inserts this record and returns the persisted value."]
            pub fn save(self) -> Result<#name<seekwel::Persisted>, seekwel::error::Error> {
                let id = seekwel::model::insert(&self)?;
                Ok(#name {
                    id,
                    #(#col_names: self.#col_names,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }
        }

        #[doc = concat!("Builder for [`", stringify!(#name), "<seekwel::NewRecord>`].")]
        #vis struct #builder_name {
            #(#builder_fields,)*
        }

        impl #builder_name {
            #(#builder_setters)*

            #[doc = concat!("Builds [`", stringify!(#name), "<seekwel::NewRecord>`].")]
            pub fn build(self) -> Result<#name<seekwel::NewRecord>, seekwel::error::Error> {
                #(#build_extracts)*

                Ok(#name {
                    id: 0,
                    #(#col_names,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }

            #[doc = concat!("Builds and inserts [`", stringify!(#name), "<seekwel::Persisted>`].")]
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

fn analyze_model_field(field: &Field) -> Result<ModelField<'_>, syn::Error> {
    let ident = field.ident.as_ref().unwrap();
    let field_name = ident_name(ident);
    let optional_inner_ty = option_inner_type(&field.ty);
    let relation_target = belongs_to_target_type(&field.ty)
        .or_else(|| optional_inner_ty.and_then(belongs_to_target_type));

    if let Some(target) = relation_target {
        validate_relation_target_type(target)?;
    }

    let storage_column_name = if relation_target.is_some() {
        format!("{field_name}_id")
    } else {
        field_name.clone()
    };

    Ok(ModelField {
        ident,
        ty: &field.ty,
        field_name,
        query_variant: column_variant_ident_from_str(&storage_column_name),
        storage_column_name,
        is_optional: optional_inner_ty.is_some(),
        optional_inner_ty,
        relation_target,
    })
}

fn validate_relation_target_type(ty: &Type) -> Result<(), syn::Error> {
    if is_option_type(ty) {
        return Err(syn::Error::new_spanned(
            ty,
            "BelongsTo<Option<T>> is not supported; use Option<BelongsTo<T>> instead",
        ));
    }

    Ok(())
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

fn belongs_to_target_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "BelongsTo"
            && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return Some(inner);
        }
    }
    None
}

fn ident_name(ident: &Ident) -> String {
    let raw = ident.to_string();
    raw.strip_prefix("r#").unwrap_or(&raw).to_string()
}

fn column_variant_ident_from_str(raw: &str) -> Ident {
    let mut variant = String::new();
    for part in raw.split('_').filter(|part| !part.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            variant.extend(first.to_uppercase());
            variant.extend(chars);
        }
    }

    if variant.is_empty() {
        variant.push_str("Column");
    }

    format_ident!("{}", variant)
}
