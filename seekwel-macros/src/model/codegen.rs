//! Generated `from_row` reads column 0 as the primary key and columns 1..N as
//! the stored fields in declaration order; the SELECT/INSERT column lists must
//! keep that order to match.

use quote::{format_ident, quote};

use super::ir::{ModelFieldSpec, ModelSpec};

pub(crate) fn expand_model(spec: &ModelSpec) -> proc_macro2::TokenStream {
    let name = &spec.name;
    let vis = &spec.vis;
    let table_name = &spec.table_name;
    let builder_name = &spec.builder_name;
    let params_name = format_ident!("{}Params", name);
    let allowed_params_name = format_ident!("Allowed{}Params", name);
    let columns_name = &spec.columns_name;
    let state_field_name = &spec.state_field_name;
    let validator = &spec.validator;
    let primary_key = &spec.primary_key;
    let primary_key_ident = &primary_key.ident;
    let primary_key_ty = &primary_key.ty;
    let primary_key_name = primary_key.field_name.as_str();
    let primary_key_variant = &primary_key.query_variant;
    let primary_key_auto_increment = primary_key.auto_increment;
    let registry_suffix = name.to_string().to_ascii_lowercase();
    let registry_fn_name = format_ident!("__seekwel_registry_table_for_{}", registry_suffix);
    let registry_entry_name = format_ident!(
        "__SEEKWEL_REGISTRY_ENTRY_FOR_{}",
        registry_suffix.to_ascii_uppercase()
    );

    let stored_fields: Vec<_> = spec.stored_fields().collect();
    let has_many_fields: Vec<_> = spec.has_many_fields().collect();

    let column_variants: Vec<_> = stored_fields
        .iter()
        .map(|field| &field.query_variant)
        .collect();
    let column_names: Vec<_> = stored_fields
        .iter()
        .map(|field| field.storage_column_name.as_str())
        .collect();
    let column_variant_docs = column_names.iter().map(|column_name| {
        quote! {
            #[doc = concat!("The `", #column_name, "` column.")]
        }
    });
    let association_key_consts: Vec<_> = stored_fields
        .iter()
        .map(|field| &field.association_key_const)
        .collect();
    let association_key_docs = column_names.iter().map(|column_name| {
        quote! {
            #[doc = concat!("Const-generic association key for the `", #column_name, "` column.")]
        }
    });
    let association_key_values = (0..stored_fields.len()).map(|index| index as u8);
    let column_defs: Vec<_> = stored_fields
        .iter()
        .map(|field| {
            let column_name = field.storage_column_name.as_str();
            let ty = &field.ty;
            quote! {
                seekwel::model::column::<#ty>(#column_name)
            }
        })
        .collect();
    let insert_column_defs = if primary_key_auto_increment {
        quote! { #(#column_defs,)* }
    } else {
        quote! {
            seekwel::model::column::<#primary_key_ty>(#primary_key_name),
            #(#column_defs,)*
        }
    };

    let from_row_fields = stored_fields.iter().enumerate().map(|(index, field)| {
        let field_name = &field.ident;
        let ty = &field.ty;
        let row_index = index + 1;
        quote! {
            #field_name: <#ty as seekwel::model::SqlField>::from_sql_row(row, #row_index)?
        }
    });
    let from_row_has_many_fields = has_many_fields.iter().map(|field| {
        let field_name = &field.ident;
        quote! {
            #field_name: seekwel::HasMany::new_bound(__seekwel_association_id)
        }
    });

    let param_exprs: Vec<_> = stored_fields
        .iter()
        .map(|field| {
            let field_name = &field.ident;
            let ty = &field.ty;
            quote! {
                <#ty as seekwel::model::SqlField>::to_sql_value(&self.#field_name)
            }
        })
        .collect();
    let insert_param_exprs = if primary_key_auto_increment {
        quote! { #(#param_exprs,)* }
    } else {
        quote! {
            <#primary_key_ty as seekwel::model::SqlField>::to_sql_value(&self.#primary_key_ident),
            #(#param_exprs,)*
        }
    };

    #[cfg(feature = "serde")]
    let params_derives = quote! {
        #[derive(Clone, Default, seekwel::__private::serde::Deserialize)]
    };
    #[cfg(not(feature = "serde"))]
    let params_derives = quote! {
        #[derive(Clone, Default)]
    };
    #[cfg(feature = "serde")]
    let params_serde_default = quote! { #[serde(default)] };
    #[cfg(not(feature = "serde"))]
    let params_serde_default = quote! {};

    let mut builder_fields = Vec::<proc_macro2::TokenStream>::new();
    let mut builder_defaults = Vec::<proc_macro2::TokenStream>::new();
    let mut builder_setters = Vec::<proc_macro2::TokenStream>::new();
    let mut build_extracts = Vec::<proc_macro2::TokenStream>::new();

    if !primary_key_auto_increment {
        builder_fields.push(quote! {
            #primary_key_ident: seekwel::model::builder::Required<#primary_key_ty>
        });
        builder_defaults.push(quote! { #primary_key_ident: Default::default() });
        builder_setters.push(quote! {
            #[doc = concat!("Sets the `", #primary_key_name, "` field.")]
            pub fn #primary_key_ident(mut self, value: impl Into<#primary_key_ty>) -> Self {
                self.#primary_key_ident.set(value);
                self
            }
        });
        build_extracts.push(quote! {
            let #primary_key_ident = self.#primary_key_ident.finish(#primary_key_name)?;
        });
    }

    for field in &stored_fields {
        let field_name = &field.ident;
        let field_name_str = field.field_name.as_str();
        if let Some(inner_ty) = field.optional_inner_ty.as_ref() {
            builder_fields
                .push(quote! { #field_name: seekwel::model::builder::Optional<#inner_ty> });
        } else {
            let ty = &field.ty;
            builder_fields.push(quote! { #field_name: seekwel::model::builder::Required<#ty> });
        }
        builder_defaults.push(quote! { #field_name: Default::default() });

        if field.is_optional {
            if let Some(association_ty) = field
                .optional_inner_ty
                .as_ref()
                .filter(|_| field.association_target.is_some())
            {
                builder_setters.push(quote! {
                    #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                    pub fn #field_name<V>(mut self, value: Option<V>) -> Self
                    where
                        V: Into<#association_ty>,
                    {
                        self.#field_name.set(value.map(Into::into));
                        self
                    }
                });
            } else {
                let ty = &field.ty;
                builder_setters.push(quote! {
                    #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                    pub fn #field_name(mut self, value: #ty) -> Self {
                        self.#field_name.set(value);
                        self
                    }
                });
            }
            build_extracts.push(quote! { let #field_name = self.#field_name.finish(); });
        } else {
            let ty = &field.ty;
            builder_setters.push(quote! {
                #[doc = concat!("Sets the `", #field_name_str, "` field.")]
                pub fn #field_name(mut self, value: impl Into<#ty>) -> Self {
                    self.#field_name.set(value);
                    self
                }
            });
            build_extracts.push(quote! {
                let #field_name = self.#field_name.finish(#field_name_str)?;
            });
        }
    }

    let primary_key_params_field = if primary_key_auto_increment {
        quote! {}
    } else {
        quote! {
            #params_serde_default
            #primary_key_ident: seekwel::model::params::Param<#primary_key_ty>,
        }
    };
    let primary_key_params_setter = if primary_key_auto_increment {
        quote! {}
    } else {
        quote! {
            #[doc = concat!("Sets the `", #primary_key_name, "` params field.")]
            pub fn #primary_key_ident(mut self, value: impl Into<#primary_key_ty>) -> Self {
                self.#primary_key_ident = seekwel::model::params::Param::provided(value.into());
                self
            }
        }
    };
    let primary_key_params_new_extract = if primary_key_auto_increment {
        quote! {}
    } else {
        quote! {
            let #primary_key_ident = if __seekwel_is_allowed(#columns_name::#primary_key_variant) {
                __seekwel_params
                    .#primary_key_ident
                    .into_value()
                    .ok_or_else(|| seekwel::error::Error::MissingField(#primary_key_name.to_string()))?
            } else {
                return Err(seekwel::error::Error::MissingField(#primary_key_name.to_string()));
            };
        }
    };
    let primary_key_params_new_init = if primary_key_auto_increment {
        quote! { #primary_key_ident: Default::default() }
    } else {
        quote! { #primary_key_ident }
    };
    let auto_primary_key_params_validation = if primary_key_auto_increment {
        quote! {
            if __seekwel_is_allowed(#columns_name::#primary_key_variant) {
                return Err(seekwel::error::Error::InvalidParams(format!(
                    "column `{}` is not assignable from params",
                    #primary_key_name,
                )));
            }
        }
    } else {
        quote! {}
    };
    let primary_key_update_params_validation = quote! {
        if __seekwel_is_allowed(#columns_name::#primary_key_variant) {
            return Err(seekwel::error::Error::InvalidParams(format!(
                "column `{}` is not assignable by update params",
                #primary_key_name,
            )));
        }
    };
    let params_allow_all_columns = if primary_key_auto_increment {
        quote! { vec![#(#columns_name::#column_variants,)*] }
    } else {
        quote! { vec![#columns_name::#primary_key_variant, #(#columns_name::#column_variants,)*] }
    };

    let mut params_fields = Vec::<proc_macro2::TokenStream>::new();
    let mut params_setters = Vec::<proc_macro2::TokenStream>::new();
    let mut params_new_extracts = Vec::<proc_macro2::TokenStream>::new();
    let mut params_update_assignments = Vec::<proc_macro2::TokenStream>::new();

    for field in &stored_fields {
        let model_field_name = &field.ident;
        let param_field_name = if field.association_target.is_some() {
            format_ident!("{}", field.storage_column_name.as_str())
        } else {
            field.ident.clone()
        };
        let param_field_name_str = field.storage_column_name.as_str();
        let column_variant = &field.query_variant;
        let ty = &field.ty;

        params_fields.push(quote! {
            #params_serde_default
            #param_field_name: seekwel::model::params::Param<#ty>,
        });

        if field.is_optional {
            if let Some(association_ty) = field
                .optional_inner_ty
                .as_ref()
                .filter(|_| field.association_target.is_some())
            {
                params_setters.push(quote! {
                    #[doc = concat!("Sets the `", #param_field_name_str, "` params field.")]
                    pub fn #param_field_name<V>(mut self, value: Option<V>) -> Self
                    where
                        V: Into<#association_ty>,
                    {
                        self.#param_field_name =
                            seekwel::model::params::Param::provided(value.map(Into::into));
                        self
                    }
                });
            } else {
                params_setters.push(quote! {
                    #[doc = concat!("Sets the `", #param_field_name_str, "` params field.")]
                    pub fn #param_field_name(mut self, value: #ty) -> Self {
                        self.#param_field_name = seekwel::model::params::Param::provided(value);
                        self
                    }
                });
            }
            params_new_extracts.push(quote! {
                let #model_field_name = if __seekwel_is_allowed(#columns_name::#column_variant) {
                    __seekwel_params.#param_field_name.into_value().unwrap_or(None)
                } else {
                    None
                };
            });
        } else {
            params_setters.push(quote! {
                #[doc = concat!("Sets the `", #param_field_name_str, "` params field.")]
                pub fn #param_field_name(mut self, value: impl Into<#ty>) -> Self {
                    self.#param_field_name = seekwel::model::params::Param::provided(value.into());
                    self
                }
            });
            params_new_extracts.push(quote! {
                let #model_field_name = if __seekwel_is_allowed(#columns_name::#column_variant) {
                    __seekwel_params
                        .#param_field_name
                        .into_value()
                        .ok_or_else(|| seekwel::error::Error::MissingField(#param_field_name_str.to_string()))?
                } else {
                    return Err(seekwel::error::Error::MissingField(#param_field_name_str.to_string()));
                };
            });
        }

        params_update_assignments.push(quote! {
            if __seekwel_is_allowed(#columns_name::#column_variant) {
                if let Some(__seekwel_value) = __seekwel_params.#param_field_name.into_value() {
                    self.#model_field_name = __seekwel_value;
                }
            }
        });
    }

    let new_record_field_inits: Vec<_> = spec
        .fields
        .iter()
        .map(|field| match field {
            ModelFieldSpec::Stored(field) => {
                let field_name = &field.ident;
                quote! { #field_name }
            }
            ModelFieldSpec::HasMany(field) => {
                let field_name = &field.ident;
                quote! { #field_name: seekwel::HasMany::new_unbound() }
            }
        })
        .collect();

    let persisted_field_inits = spec.fields.iter().map(|field| match field {
        ModelFieldSpec::Stored(field) => {
            let field_name = &field.ident;
            quote! { #field_name: self.#field_name }
        }
        ModelFieldSpec::HasMany(field) => {
            let field_name = &field.ident;
            quote! { #field_name: seekwel::HasMany::new_bound(__seekwel_association_id) }
        }
    });

    let invalid_new_field_inits = spec.fields.iter().map(|field| match field {
        ModelFieldSpec::Stored(field) => {
            let field_name = &field.ident;
            quote! { #field_name: self.#field_name }
        }
        ModelFieldSpec::HasMany(field) => {
            let field_name = &field.ident;
            quote! { #field_name: self.#field_name }
        }
    });

    let invalid_persisted_field_inits = spec.fields.iter().map(|field| match field {
        ModelFieldSpec::Stored(field) => {
            let field_name = &field.ident;
            quote! { #field_name: self.#field_name.clone() }
        }
        ModelFieldSpec::HasMany(field) => {
            let field_name = &field.ident;
            quote! { #field_name: self.#field_name.clone() }
        }
    });

    let belongs_to_methods = stored_fields.iter().filter_map(|field| {
        let association_target = field.association_target.as_ref()?;
        let field_name = &field.ident;
        let field_name_str = field.field_name.as_str();

        Some(if field.is_optional {
            quote! {
                #[doc = concat!("Loads the `", #field_name_str, "` association.")]
                pub fn #field_name(&self) -> Result<Option<#association_target>, seekwel::error::Error>
                where
                    #association_target: seekwel::model::PersistedModel + Clone,
                {
                    self.#field_name
                        .as_ref()
                        .map(|association| association.load())
                        .transpose()
                }
            }
        } else {
            quote! {
                #[doc = concat!("Loads the `", #field_name_str, "` association.")]
                pub fn #field_name(&self) -> Result<#association_target, seekwel::error::Error>
                where
                    #association_target: seekwel::model::PersistedModel + Clone,
                {
                    self.#field_name.load()
                }
            }
        })
    });

    let has_many_methods = has_many_fields.iter().map(|field| {
        let field_name = &field.ident;
        let field_name_str = field.field_name.as_str();
        let child_ty = &field.child_ty;
        let association_key_path = &field.association_key_path;
        quote! {
            #[doc = concat!("Loads the `", #field_name_str, "` association.")]
            pub fn #field_name(&self) -> Result<Vec<#child_ty>, seekwel::error::Error>
            where
                #child_ty: seekwel::model::HasManyAssociation<{ #association_key_path }, Parent = #name<seekwel::Persisted>>,
            {
                self.#field_name.load()
            }
        }
    });

    let has_many_validations = has_many_fields.iter().map(|field| {
        let child_ty = &field.child_ty;
        let association_key_path = &field.association_key_path;
        quote! {
            const _: () = {
                struct AssertHasMany
                where
                    #child_ty: seekwel::model::HasManyAssociation<{ #association_key_path }, Parent = #name<seekwel::Persisted>>;
            };
        }
    });

    let has_many_association_impls = stored_fields.iter().filter_map(|field| {
        let parent = field.association_target.as_ref()?;
        let query_variant = &field.query_variant;
        let association_key_const = &field.association_key_const;
        let setter = &field.ident;
        let append_call = if field.is_optional {
            quote! { Ok(builder.#setter(Some(parent_id)).create()?) }
        } else {
            quote! { Ok(builder.#setter(parent_id).create()?) }
        };

        Some(quote! {
            impl seekwel::model::HasManyAssociation<{ #columns_name::#association_key_const }> for #name<seekwel::Persisted> {
                type Parent = #parent;
                type Builder = #builder_name;

                fn load_for_parent(parent_id: u64) -> Result<Vec<Self>, seekwel::error::Error> {
                    <seekwel::model::Query<Self> as seekwel::model::QueryDsl>::all(
                        seekwel::model::Query::new(
                            #columns_name::#query_variant,
                            seekwel::Comparison::Eq(parent_id),
                        )
                    )
                }

                fn append_for_parent(
                    parent_id: u64,
                    builder: Self::Builder,
                ) -> Result<Self, seekwel::error::Error> {
                    #append_call
                }
            }
        })
    });

    let new_record_primary_key_init = if primary_key_auto_increment {
        quote! { #primary_key_ident: Default::default() }
    } else {
        quote! { #primary_key_ident }
    };
    let persisted_primary_key_expr = if primary_key_auto_increment {
        quote! {
            <#primary_key_ty as seekwel::model::PrimaryKeyField>::from_generated_id(id)?
        }
    } else {
        quote! { self.#primary_key_ident }
    };

    let (impl_generics, ty_generics, where_clause) = spec.generics.split_for_impl();

    quote! {
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #registry_fn_name() -> Option<seekwel::schema::TableDef> {
            Some(seekwel::schema::__private::table_for_model::<#name<seekwel::Persisted>>())
        }

        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        #[doc(hidden)]
        #[used]
        #[allow(non_upper_case_globals)]
        #[unsafe(link_section = "seekwel_schema_registry")]
        static #registry_entry_name: seekwel::schema::__private::RegistryEntry =
            seekwel::schema::__private::RegistryEntry::new(#registry_fn_name);

        #[doc = concat!("Typed columns for [`", stringify!(#name), "`].")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #vis enum #columns_name {
            #[doc = concat!("The `", #primary_key_name, "` column.")]
            #primary_key_variant,
            #(#column_variant_docs #column_variants,)*
        }

        impl #columns_name {
            #(#association_key_docs
            pub const #association_key_consts: u8 = #association_key_values;)*

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
                    Self::#primary_key_variant => #primary_key_name,
                    #(Self::#column_variants => #column_names,)*
                }
            }
        }

        #[doc = concat!("Params object for [`", stringify!(#name), "`] assignment.")]
        #params_derives
        #vis struct #params_name {
            #primary_key_params_field
            #(#params_fields)*
        }

        impl #params_name {
            #[doc = "Creates an empty params object."]
            pub fn new() -> Self {
                Self::default()
            }

            #primary_key_params_setter
            #(#params_setters)*

            #[doc = "Keeps only the listed columns available for model assignment."]
            pub fn allow<I>(self, columns: I) -> #allowed_params_name
            where
                I: IntoIterator<Item = #columns_name>,
            {
                <Self as seekwel::model::Params>::allow(self, columns)
            }

            #[doc = "Keeps every column generated for this params object available for model assignment."]
            pub fn allow_all(self) -> #allowed_params_name {
                <Self as seekwel::model::Params>::allow_all(self)
            }
        }

        impl seekwel::model::Params for #params_name {
            type Model = #name<seekwel::Persisted>;
            type Allowed = #allowed_params_name;

            fn allow<I>(self, columns: I) -> Self::Allowed
            where
                I: IntoIterator<Item = <Self::Model as seekwel::model::Model>::Column>,
            {
                #allowed_params_name {
                    params: self,
                    allowed: columns.into_iter().collect(),
                }
            }

            fn allow_all(self) -> Self::Allowed {
                #allowed_params_name {
                    params: self,
                    allowed: #params_allow_all_columns,
                }
            }
        }

        #[doc = concat!("Filtered params object for [`", stringify!(#name), "`] assignment.")]
        #vis struct #allowed_params_name {
            params: #params_name,
            allowed: std::vec::Vec<#columns_name>,
        }

        impl #allowed_params_name {
            #[doc = "Returns whether a column is available for model assignment."]
            pub fn allows(&self, column: #columns_name) -> bool {
                self.allowed.contains(&column)
            }
        }

        #(#has_many_validations)*
        #(#has_many_association_impls)*

        impl #impl_generics seekwel::model::Model for #name #ty_generics #where_clause {
            type Column = #columns_name;

            fn table_name() -> &'static str {
                #table_name
            }

            fn primary_key() -> seekwel::model::PrimaryKeyDef {
                seekwel::model::PrimaryKeyDef {
                    name: #primary_key_name,
                    sql_type: <#primary_key_ty as seekwel::model::SqlField>::SQL_TYPE,
                    auto_increment: #primary_key_auto_increment,
                }
            }

            fn columns() -> &'static [seekwel::model::ColumnDef] {
                const COLUMNS: &[seekwel::model::ColumnDef] = &[#(#column_defs,)*];
                COLUMNS
            }

            fn insert_columns() -> &'static [seekwel::model::ColumnDef] {
                const INSERT_COLUMNS: &[seekwel::model::ColumnDef] = &[#insert_column_defs];
                INSERT_COLUMNS
            }

            fn params(&self) -> Vec<rusqlite::types::Value> {
                vec![#(#param_exprs,)*]
            }

            fn insert_params(&self) -> Vec<rusqlite::types::Value> {
                vec![#insert_param_exprs]
            }
        }

        impl seekwel::model::ModelRecord for #name<seekwel::NewRecord> {
            fn persisted_id(&self) -> Option<u64> {
                None
            }

            fn persisted_primary_key_value(&self) -> Option<rusqlite::types::Value> {
                None
            }
        }

        impl seekwel::model::ModelRecord for #name<seekwel::Persisted> {
            fn persisted_id(&self) -> Option<u64> {
                Some(
                    <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(
                        &self.#primary_key_ident,
                    )
                    .expect("seekwel persisted model primary key should convert to a non-negative u64 association id"),
                )
            }

            fn persisted_primary_key_value(&self) -> Option<rusqlite::types::Value> {
                Some(<#primary_key_ty as seekwel::model::SqlField>::to_sql_value(
                    &self.#primary_key_ident,
                ))
            }
        }

        impl seekwel::model::ModelRecord for #name<seekwel::Invalid<seekwel::NewRecord, #columns_name>> {
            fn persisted_id(&self) -> Option<u64> {
                None
            }

            fn persisted_primary_key_value(&self) -> Option<rusqlite::types::Value> {
                None
            }
        }

        impl seekwel::model::ModelRecord for #name<seekwel::Invalid<seekwel::Persisted, #columns_name>> {
            fn persisted_id(&self) -> Option<u64> {
                Some(
                    <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(
                        &self.#primary_key_ident,
                    )
                    .expect("seekwel invalid persisted model primary key should convert to a non-negative u64 association id"),
                )
            }

            fn persisted_primary_key_value(&self) -> Option<rusqlite::types::Value> {
                Some(<#primary_key_ty as seekwel::model::SqlField>::to_sql_value(
                    &self.#primary_key_ident,
                ))
            }
        }

        impl seekwel::model::PersistedModel for #name<seekwel::Persisted> {
            fn id(&self) -> u64 {
                <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(&self.#primary_key_ident)
                    .expect("seekwel persisted model primary key should convert to a non-negative u64 association id")
            }

            fn primary_key_value(&self) -> rusqlite::types::Value {
                <#primary_key_ty as seekwel::model::SqlField>::to_sql_value(&self.#primary_key_ident)
            }

            fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
                let #primary_key_ident = <#primary_key_ty as seekwel::model::SqlField>::from_sql_row(row, 0)?;
                let __seekwel_association_id =
                    <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(&#primary_key_ident)
                        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                Ok(Self {
                    #primary_key_ident,
                    #(#from_row_fields,)*
                    #(#from_row_has_many_fields,)*
                    #state_field_name: seekwel::Persisted,
                })
            }

            type Invalid = #name<seekwel::Invalid<seekwel::Persisted, #columns_name>>;

            fn validation_errors(&self) -> seekwel::Errors<Self::Column> {
                let mut errors = seekwel::Errors::new();
                <#validator as seekwel::model::Validator<Self>>::validate(self, &mut errors);
                errors
            }

            fn to_invalid(&self, errors: seekwel::Errors<Self::Column>) -> Self::Invalid {
                #name {
                    #primary_key_ident: self.#primary_key_ident.clone(),
                    #(#invalid_persisted_field_inits,)*
                    #state_field_name: seekwel::Invalid::new(seekwel::Persisted, errors),
                }
            }
        }

        impl seekwel::model::NewModel for #name<seekwel::NewRecord> {
            type Persisted = #name<seekwel::Persisted>;
            type Invalid = #name<seekwel::Invalid<seekwel::NewRecord, #columns_name>>;

            fn validation_errors(&self) -> seekwel::Errors<Self::Column> {
                let mut errors = seekwel::Errors::new();
                <#validator as seekwel::model::Validator<Self>>::validate(self, &mut errors);
                errors
            }

            fn into_invalid(self, errors: seekwel::Errors<Self::Column>) -> Self::Invalid {
                #name {
                    #primary_key_ident: self.#primary_key_ident,
                    #(#invalid_new_field_inits,)*
                    #state_field_name: seekwel::Invalid::new(seekwel::NewRecord, errors),
                }
            }

            fn into_persisted(
                self,
                id: u64,
            ) -> Result<Self::Persisted, seekwel::error::Error> {
                let __seekwel_primary_key: #primary_key_ty = #persisted_primary_key_expr;
                let __seekwel_association_id =
                    <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(&__seekwel_primary_key)?;
                Ok(#name {
                    #primary_key_ident: __seekwel_primary_key,
                    #(#persisted_field_inits,)*
                    #state_field_name: seekwel::Persisted,
                })
            }
        }

        impl<S> seekwel::model::InvalidModel for #name<seekwel::Invalid<S, #columns_name>> {
            type PreviousState = S;

            fn errors(&self) -> &seekwel::Errors<Self::Column> {
                self.#state_field_name.errors()
            }
        }

        impl seekwel::model::params::ParamsModel for #name<seekwel::Persisted> {
            type NewRecord = #name<seekwel::NewRecord>;
            type Params = #params_name;

            fn build_from_params(
                __seekwel_allowed_params: #allowed_params_name,
            ) -> Result<Self::NewRecord, seekwel::error::Error> {
                let #allowed_params_name {
                    params: __seekwel_params,
                    allowed: __seekwel_allowed,
                } = __seekwel_allowed_params;
                let __seekwel_is_allowed = |column: #columns_name| __seekwel_allowed.contains(&column);

                #auto_primary_key_params_validation
                #primary_key_params_new_extract
                #(#params_new_extracts)*

                Ok(#name {
                    #primary_key_params_new_init,
                    #(#new_record_field_inits,)*
                    #state_field_name: seekwel::NewRecord,
                })
            }

            fn apply_params(
                &mut self,
                __seekwel_allowed_params: #allowed_params_name,
            ) -> Result<(), seekwel::error::Error> {
                let #allowed_params_name {
                    params: __seekwel_params,
                    allowed: __seekwel_allowed,
                } = __seekwel_allowed_params;
                let __seekwel_is_allowed = |column: #columns_name| __seekwel_allowed.contains(&column);

                #primary_key_update_params_validation
                #(#params_update_assignments)*

                Ok(())
            }
        }

        impl #name<seekwel::Persisted> {
            #[doc = concat!("Creates a builder for [`", stringify!(#name), "<seekwel::NewRecord>`].")]
            pub fn builder() -> #builder_name {
                #builder_name {
                    #(#builder_defaults,)*
                }
            }

            #(#belongs_to_methods)*
            #(#has_many_methods)*
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
                    #new_record_primary_key_init,
                    #(#new_record_field_inits,)*
                    #state_field_name: seekwel::NewRecord,
                })
            }

            #[doc = concat!("Builds and inserts [`", stringify!(#name), "<seekwel::Persisted>`].")]
            pub fn create(
                self,
            ) -> Result<
                #name<seekwel::Persisted>,
                seekwel::model::SaveError<<#name<seekwel::NewRecord> as seekwel::model::NewModel>::Invalid>,
            > {
                <#name<seekwel::NewRecord> as seekwel::model::NewModel>::save(
                    self.build().map_err(seekwel::model::SaveError::Error)?,
                )
            }
        }
    }
}
