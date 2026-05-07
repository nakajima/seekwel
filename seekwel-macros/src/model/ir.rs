use syn::{Generics, Ident, Path, Type, Visibility};

pub(crate) struct ModelSpec {
    pub(crate) name: Ident,
    pub(crate) vis: Visibility,
    pub(crate) table_name: String,
    pub(crate) builder_name: Ident,
    pub(crate) columns_name: Ident,
    pub(crate) generics: Generics,
    pub(crate) state_field_name: Ident,
    pub(crate) primary_key: PrimaryKeySpec,
    pub(crate) fields: Vec<ModelFieldSpec>,
}

impl ModelSpec {
    pub(crate) fn stored_fields(&self) -> impl Iterator<Item = &StoredFieldSpec> {
        self.fields.iter().filter_map(|field| match field {
            ModelFieldSpec::Stored(field) => Some(field),
            ModelFieldSpec::HasMany(_) => None,
        })
    }

    pub(crate) fn has_many_fields(&self) -> impl Iterator<Item = &HasManyFieldSpec> {
        self.fields.iter().filter_map(|field| match field {
            ModelFieldSpec::Stored(_) => None,
            ModelFieldSpec::HasMany(field) => Some(field),
        })
    }
}

// Built once per model at macro-expansion time and dropped immediately;
// boxing the larger variant would only add indirection.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ModelFieldSpec {
    Stored(StoredFieldSpec),
    HasMany(HasManyFieldSpec),
}

pub(crate) struct PrimaryKeySpec {
    pub(crate) ident: Ident,
    pub(crate) ty: Type,
    pub(crate) field_name: String,
    pub(crate) query_variant: Ident,
    pub(crate) auto_increment: bool,
}

pub(crate) struct StoredFieldSpec {
    pub(crate) ident: Ident,
    pub(crate) ty: Type,
    pub(crate) field_name: String,
    pub(crate) storage_column_name: String,
    pub(crate) query_variant: Ident,
    pub(crate) association_key_const: Ident,
    pub(crate) is_optional: bool,
    pub(crate) optional_inner_ty: Option<Type>,
    pub(crate) association_target: Option<Type>,
}

pub(crate) struct HasManyFieldSpec {
    pub(crate) ident: Ident,
    pub(crate) field_name: String,
    pub(crate) child_ty: Type,
    pub(crate) association_key_path: Path,
}
