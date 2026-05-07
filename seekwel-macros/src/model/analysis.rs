use quote::format_ident;
use syn::{Attribute, Data, DeriveInput, Expr, Field, Fields, GenericParam, Ident, Lit, Path, Type};

use super::ir::{HasManyFieldSpec, ModelFieldSpec, ModelSpec, PrimaryKeySpec, StoredFieldSpec};

struct ModelConfig {
    table_name: Option<String>,
    primary_key: Option<String>,
    auto_increment: Option<bool>,
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
        .filter(|field| is_phantom_data_type(&field.ty))
        .collect();
    if state_fields.len() != 1 {
        return Err(syn::Error::new_spanned(
            &name,
            "Model structs must contain exactly one PhantomData typestate field; use #[seekwel::model] to have it injected automatically",
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
            ident != &pk_ident && !is_phantom_data_type(&field.ty)
        })
        .map(analyze_model_field)
        .collect::<Result<Vec<_>, _>>()?;

    let stored_field_count = model_fields
        .iter()
        .filter(|field| matches!(field, ModelFieldSpec::Stored(_)))
        .count();
    if stored_field_count > (u8::MAX as usize + 1) {
        return Err(syn::Error::new_spanned(
            &name,
            "Model structs support at most 256 stored fields because HasMany association keys are encoded as u8",
        ));
    }

    Ok(ModelSpec {
        name,
        vis,
        table_name,
        builder_name,
        columns_name,
        generics: input.generics,
        state_field_name,
        primary_key,
        fields: model_fields,
    })
}

fn parse_model_config(attrs: &[Attribute]) -> Result<ModelConfig, syn::Error> {
    let mut config = ModelConfig {
        table_name: None,
        primary_key: None,
        auto_increment: None,
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

            Err(meta.error(
                "unsupported seekwel model option; expected `table_name`, `primary_key`, or `auto_increment`",
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

    if let Some(target) = association_target.as_ref() {
        validate_association_target_type(target)?;
    }

    let storage_column_name = if association_target.is_some() {
        format!("{field_name}_id")
    } else {
        field_name.clone()
    };

    Ok(ModelFieldSpec::Stored(StoredFieldSpec {
        ident,
        ty: field.ty.clone(),
        field_name,
        query_variant: column_variant_ident_from_str(&storage_column_name),
        association_key_const: constant_ident_from_str(&storage_column_name),
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
            "Option<HasMany<T, C>> is not supported",
        ));
    }

    let Some((child_ty, association_key_path)) = has_many_type(&field.ty)? else {
        return Ok(None);
    };

    Ok(Some(HasManyFieldSpec {
        ident: field.ident.as_ref().unwrap().clone(),
        field_name,
        child_ty,
        association_key_path,
    }))
}

fn has_many_type(ty: &Type) -> Result<Option<(Type, Path)>, syn::Error> {
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
            "HasMany fields must use `HasMany<Child, { ChildColumns::ASSOCIATION_ID }>`",
        ));
    };

    if args.args.len() != 2 {
        return Err(syn::Error::new_spanned(
            ty,
            "HasMany fields must use exactly two generic arguments: `HasMany<Child, { ChildColumns::ASSOCIATION_ID }>`",
        ));
    }

    let child_ty = match args.args.first() {
        Some(syn::GenericArgument::Type(ty)) => ty.clone(),
        Some(other) => {
            return Err(syn::Error::new_spanned(
                other,
                "The first HasMany argument must be the child model type",
            ));
        }
        None => unreachable!(),
    };

    let association_expr = match args.args.iter().nth(1) {
        Some(syn::GenericArgument::Const(expr)) => expr,
        Some(other) => {
            return Err(syn::Error::new_spanned(
                other,
                "The second HasMany argument must be a const association key like `{ PetColumns::OWNER_ID }`",
            ));
        }
        None => unreachable!(),
    };

    let Some(association_key_path) = const_expr_path(association_expr).cloned() else {
        return Err(syn::Error::new_spanned(
            association_expr,
            "The second HasMany argument must be a const association key path like `{ PetColumns::OWNER_ID }`",
        ));
    };

    Ok(Some((child_ty, association_key_path)))
}

fn const_expr_path(expr: &Expr) -> Option<&Path> {
    match expr {
        Expr::Path(expr_path) => Some(&expr_path.path),
        Expr::Block(expr_block) => match expr_block.block.stmts.as_slice() {
            [syn::Stmt::Expr(expr, None)] => const_expr_path(expr),
            _ => None,
        },
        _ => None,
    }
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

fn constant_ident_from_str(raw: &str) -> Ident {
    let mut constant = String::new();
    for part in raw.split('_').filter(|part| !part.is_empty()) {
        if !constant.is_empty() {
            constant.push('_');
        }
        constant.push_str(&part.to_uppercase());
    }

    if constant.is_empty() {
        constant.push_str("COLUMN");
    }

    format_ident!("{}", constant)
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
