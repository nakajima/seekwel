use quote::format_ident;
use syn::{
    Attribute, Data, DeriveInput, Expr, Field, Fields, GenericParam, Ident, Lit, Path,
    PathArguments, Type, parse_quote,
};

use super::ir::{HasManyFieldSpec, ModelFieldSpec, ModelSpec, PrimaryKeySpec, StoredFieldSpec};

struct ModelConfig {
    table_name: Option<String>,
    primary_key: Option<String>,
    auto_increment: Option<bool>,
    validator: Option<Path>,
}

pub(crate) fn analyze_model(input: DeriveInput) -> Result<ModelSpec, syn::Error> {
    let name = input.ident.clone();
    let vis = input.vis.clone();
    let config = parse_model_config(&input.attrs)?;
    let table_name = config
        .table_name
        .unwrap_or_else(|| name.to_string().to_lowercase());
    let primary_key_name = config.primary_key.unwrap_or_else(|| "id".to_string());
    let auto_increment = config.auto_increment.unwrap_or(true);
    let validator = config
        .validator
        .unwrap_or_else(|| parse_quote!(seekwel::model::NoValidation));
    let builder_name = format_ident!("{}Builder", name);
    let columns_name = format_ident!("{}Columns", name);

    validate_typestate_generics(&input.generics)?;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "Model only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "Model can only be derived for structs",
            ));
        }
    };

    let pk_fields: Vec<_> = fields
        .iter()
        .filter(|field| {
            field
                .ident
                .as_ref()
                .is_some_and(|ident| ident_name(ident) == primary_key_name)
        })
        .collect();
    if pk_fields.len() != 1 {
        return Err(syn::Error::new_spanned(
            &name,
            format!(
                "Model structs must contain exactly one `{}` field",
                primary_key_name
            ),
        ));
    }
    if !is_supported_primary_key_type(&pk_fields[0].ty) {
        return Err(syn::Error::new_spanned(
            &pk_fields[0].ty,
            "Primary key fields must use a supported integer type: u64, u32, u16, u8, i64, i32, i16, or i8",
        ));
    }

    let state_fields: Vec<_> = fields
        .iter()
        .filter(|field| {
            field
                .ident
                .as_ref()
                .is_some_and(|ident| ident == "__seekwel_state")
        })
        .collect();
    if state_fields.len() != 1 {
        return Err(syn::Error::new_spanned(
            &name,
            "Model structs must contain exactly one __seekwel_state typestate field; use #[seekwel::model] to have it injected automatically",
        ));
    }
    let state_field_name = state_fields[0].ident.as_ref().unwrap().clone();

    let pk_ident = pk_fields[0].ident.as_ref().unwrap().clone();
    let primary_key = PrimaryKeySpec {
        ident: pk_ident.clone(),
        ty: pk_fields[0].ty.clone(),
        field_name: primary_key_name.clone(),
        query_variant: column_variant_ident_from_str(&primary_key_name),
        auto_increment,
    };

    let model_fields = fields
        .iter()
        .filter(|field| {
            let ident = field.ident.as_ref().unwrap();
            ident != &pk_ident && ident != &state_field_name
        })
        .map(analyze_model_field)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ModelSpec {
        name,
        vis,
        table_name,
        builder_name,
        columns_name,
        generics: input.generics,
        state_field_name,
        validator,
        primary_key,
        fields: model_fields,
    })
}

fn parse_model_config(attrs: &[Attribute]) -> Result<ModelConfig, syn::Error> {
    let mut config = ModelConfig {
        table_name: None,
        primary_key: None,
        auto_increment: None,
        validator: None,
    };

    for attr in attrs {
        if !attr.path().is_ident("seekwel") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("table_name") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                if config.table_name.is_some() {
                    return Err(meta.error("duplicate `table_name` option"));
                }
                config.table_name = Some(lit.value());
                return Ok(());
            }

            if meta.path.is_ident("primary_key") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                if config.primary_key.is_some() {
                    return Err(meta.error("duplicate `primary_key` option"));
                }
                config.primary_key = Some(lit.value());
                return Ok(());
            }

            if meta.path.is_ident("auto_increment") {
                let expr: Expr = meta.value()?.parse()?;
                let Expr::Lit(expr_lit) = expr else {
                    return Err(meta.error("`auto_increment` must be a boolean literal"));
                };
                let Lit::Bool(lit) = expr_lit.lit else {
                    return Err(meta.error("`auto_increment` must be a boolean literal"));
                };
                if config.auto_increment.is_some() {
                    return Err(meta.error("duplicate `auto_increment` option"));
                }
                config.auto_increment = Some(lit.value());
                return Ok(());
            }

            if meta.path.is_ident("validator") {
                let validator: Path = meta.value()?.parse()?;
                if config.validator.is_some() {
                    return Err(meta.error("duplicate `validator` option"));
                }
                config.validator = Some(validator);
                return Ok(());
            }

            Err(meta.error(
                "unsupported seekwel model option; expected `table_name`, `primary_key`, `auto_increment`, or `validator`",
            ))
        })?;
    }

    Ok(config)
}

fn analyze_model_field(field: &Field) -> Result<ModelFieldSpec, syn::Error> {
    let ident = field.ident.as_ref().unwrap().clone();
    let field_name = ident_name(&ident);

    if let Some(has_many) = has_many_field(field, field_name.clone())? {
        return Ok(ModelFieldSpec::HasMany(has_many));
    }

    let optional_inner_ty = option_inner_type(&field.ty).cloned();
    let association_target = belongs_to_target_type(&field.ty)
        .or_else(|| optional_inner_ty.as_ref().and_then(belongs_to_target_type))
        .cloned();
    let association_key = association_key_attr(field)?;

    if let Some(target) = association_target.as_ref() {
        validate_association_target_type(target)?;
    }

    let storage_column_name = match (association_target.is_some(), association_key) {
        (true, Some(key)) => key,
        (true, None) => format!("{field_name}_id"),
        (false, Some(_)) => {
            return Err(syn::Error::new_spanned(
                field,
                "#[key = column_name] can only be used on BelongsTo or HasMany association fields",
            ));
        }
        (false, None) => field_name.clone(),
    };

    Ok(ModelFieldSpec::Stored(StoredFieldSpec {
        ident,
        ty: field.ty.clone(),
        field_name,
        query_variant: column_variant_ident_from_str(&storage_column_name),
        association_handler: association_handler_ident_from_str(&storage_column_name),
        storage_column_name,
        is_optional: optional_inner_ty.is_some(),
        optional_inner_ty,
        association_target,
    }))
}

fn has_many_field(
    field: &Field,
    field_name: String,
) -> Result<Option<HasManyFieldSpec>, syn::Error> {
    if let Some(inner) = option_inner_type(&field.ty)
        && has_many_type(inner)?.is_some()
    {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "Option<HasMany<T>> is not supported",
        ));
    }

    let Some(child_ty) = has_many_type(&field.ty)? else {
        return Ok(None);
    };
    let association_key = association_key_attr(field)?.ok_or_else(|| {
        syn::Error::new_spanned(
            field,
            "HasMany fields must specify the child foreign key with #[key = owner_id]",
        )
    })?;
    let association_handler = association_handler_ident_from_str(&association_key);

    Ok(Some(HasManyFieldSpec {
        ident: field.ident.as_ref().unwrap().clone(),
        field_name,
        child_ty,
        association_handler,
    }))
}

fn has_many_type(ty: &Type) -> Result<Option<Type>, syn::Error> {
    let Type::Path(type_path) = ty else {
        return Ok(None);
    };

    let Some(segment) = type_path.path.segments.last() else {
        return Ok(None);
    };
    if segment.ident != "HasMany" {
        return Ok(None);
    }

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "HasMany fields must use `HasMany<Child>` and #[key = child_id]",
        ));
    };

    if args.args.len() != 1 {
        return Err(syn::Error::new_spanned(
            ty,
            "HasMany fields must use exactly one generic argument: `HasMany<Child>`; put the child foreign key on the field with #[key = child_id]",
        ));
    }

    let child_ty = match args.args.first() {
        Some(syn::GenericArgument::Type(ty)) => ty.clone(),
        Some(other) => {
            return Err(syn::Error::new_spanned(
                other,
                "The HasMany argument must be the child model type",
            ));
        }
        None => unreachable!(),
    };

    Ok(Some(child_ty))
}

fn association_key_attr(field: &Field) -> Result<Option<String>, syn::Error> {
    let mut key = None;

    for attr in &field.attrs {
        if attr.path().is_ident("key") {
            if key.is_some() {
                return Err(syn::Error::new_spanned(attr, "duplicate `key` attribute"));
            }

            let syn::Meta::NameValue(meta) = &attr.meta else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "association keys must be written as #[key = column_name]",
                ));
            };

            key = Some(key_name_from_expr(&meta.value)?);
            continue;
        }

        if attr.path().is_ident("seekwel") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("key") {
                    if key.is_some() {
                        return Err(meta.error("duplicate `key` attribute"));
                    }
                    let expr: Expr = meta.value()?.parse()?;
                    key = Some(key_name_from_expr(&expr)?);
                    return Ok(());
                }

                Err(meta.error("unsupported seekwel field option; expected `key`"))
            })?;
        }
    }

    Ok(key)
}

fn key_name_from_expr(expr: &Expr) -> Result<String, syn::Error> {
    let key = match expr {
        Expr::Path(expr_path)
            if expr_path.qself.is_none()
                && expr_path.path.leading_colon.is_none()
                && expr_path.path.segments.len() == 1 =>
        {
            let segment = expr_path.path.segments.first().unwrap();
            if !matches!(segment.arguments, PathArguments::None) {
                return Err(syn::Error::new_spanned(
                    expr,
                    "association keys must be column identifiers like owner_id",
                ));
            }
            ident_name(&segment.ident)
        }
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            Lit::Str(lit) => lit.value(),
            _ => {
                return Err(syn::Error::new_spanned(
                    expr,
                    "association keys must be column identifiers like owner_id",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                expr,
                "association keys must be column identifiers like owner_id",
            ));
        }
    };

    if key.is_empty() {
        return Err(syn::Error::new_spanned(
            expr,
            "association key column names cannot be empty",
        ));
    }

    Ok(key)
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

fn validate_association_target_type(ty: &Type) -> Result<(), syn::Error> {
    if is_option_type(ty) {
        return Err(syn::Error::new_spanned(
            ty,
            "BelongsTo<Option<T>> is not supported; use Option<BelongsTo<T>> instead",
        ));
    }

    Ok(())
}

fn is_supported_primary_key_type(ty: &Type) -> bool {
    matches!(
        quote::quote!(#ty).to_string().as_str(),
        "u64" | "u32" | "u16" | "u8" | "i64" | "i32" | "i16" | "i8"
    )
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

fn association_handler_ident_from_str(raw: &str) -> Ident {
    let mut suffix = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            suffix.push(ch.to_ascii_lowercase());
        } else if !suffix.ends_with('_') {
            suffix.push('_');
        }
    }
    while suffix.ends_with('_') {
        suffix.pop();
    }
    if suffix.is_empty() {
        suffix.push_str("column");
    }

    format_ident!("__seekwel_has_many_handlers_for_{}", suffix)
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
