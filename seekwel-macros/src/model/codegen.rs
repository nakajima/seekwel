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
    let columns_name = &spec.columns_name;
    let state_field_name = &spec.state_field_name;
    let primary_key = &spec.primary_key;
    let primary_key_ident = &primary_key.ident;
    let primary_key_ty = &primary_key.ty;
    let primary_key_name = primary_key.field_name.as_str();
    let primary_key_variant = &primary_key.query_variant;
    let primary_key_auto_increment = primary_key.auto_increment;
    let registry_suffix = name.to_string().to_ascii_lowercase();
    let registry_fn_name = format_ident!("__seekwel_registry_table_for_{}", registry_suffix);
    let registry_entry_name =
        format_ident!("__SEEKWEL_REGISTRY_ENTRY_FOR_{}", registry_suffix.to_ascii_uppercase());

    let stored_fields: Vec<_> = spec.stored_fields().collect();
    let has_many_fields: Vec<_> = spec.has_many_fields().collect();

    let column_variants: Vec<_> = stored_fields.iter().map(|field| &field.query_variant).collect();
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
            builder_fields.push(quote! { #field_name: seekwel::model::builder::Optional<#inner_ty> });
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

    let new_record_field_inits = spec.fields.iter().map(|field| match field {
        ModelFieldSpec::Stored(field) => {
            let field_name = &field.ident;
            quote! { #field_name }
        }
        ModelFieldSpec::HasMany(field) => {
            let field_name = &field.ident;
            quote! { #field_name: seekwel::HasMany::new_unbound() }
        }
    });

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
            quote! { builder.#setter(Some(parent_id)).create() }
        } else {
            quote! { builder.#setter(parent_id).create() }
        };

        Some(quote! {
            impl seekwel::model::HasManyAssociation<{ #columns_name::#association_key_const }> for #name<seekwel::Persisted> {
                type Parent = #parent;
                type Builder = #builder_name;

                fn load_for_parent(parent_id: u64) -> Result<Vec<Self>, seekwel::error::Error> {
                    <seekwel::model::Query<Self> as seekwel::model::QueryDsl>::all(
                        Self::q(#columns_name::#query_variant, seekwel::Comparison::Eq(parent_id))
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
            pub fn find<K>(id: K) -> Result<Self, seekwel::error::Error>
            where
                K: seekwel::model::PrimaryKeyLookup,
            {
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

            #(#belongs_to_methods)*
            #(#has_many_methods)*
        }

        impl #name<seekwel::NewRecord> {
            #[doc = "Inserts this record and returns the persisted value."]
            pub fn save(self) -> Result<#name<seekwel::Persisted>, seekwel::error::Error> {
                let id = seekwel::model::insert(&self)?;
                let __seekwel_primary_key: #primary_key_ty = #persisted_primary_key_expr;
                let __seekwel_association_id =
                    <#primary_key_ty as seekwel::model::PrimaryKeyField>::to_association_id(&__seekwel_primary_key)?;
                Ok(#name {
                    #primary_key_ident: __seekwel_primary_key,
                    #(#persisted_field_inits,)*
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
                    #new_record_primary_key_init,
                    #(#new_record_field_inits,)*
                    #state_field_name: std::marker::PhantomData,
                })
            }

            #[doc = concat!("Builds and inserts [`", stringify!(#name), "<seekwel::Persisted>`].")]
            pub fn create(self) -> Result<#name<seekwel::Persisted>, seekwel::error::Error> {
                self.build()?.save()
            }
        }
    }
}
