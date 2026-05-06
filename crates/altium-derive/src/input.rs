use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DataStruct, DeriveInput, Field, Fields, Ident, LitStr, Path, Result, Type,
    parse_quote, parse2,
};

/// Parsed view of the input struct.
pub struct RecordInput {
    pub ident: Ident,
    pub generics: syn::Generics,
    pub record_type: Option<String>,
    /// Path to the `altium` crate. Defaults to `::altium`; override with
    /// `#[altium(crate = path::to::altium)]` when the crate is renamed or
    /// re-exported under a different name.
    pub crate_path: Path,
    pub fields: Vec<FieldInput>,
}

impl RecordInput {
    /// Tokens for the configured altium-crate path.
    pub fn crate_tokens(&self) -> TokenStream {
        let p = &self.crate_path;
        quote!(#p)
    }
}

pub struct FieldInput {
    pub ident: Ident,
    pub ty: Type,
    pub param_name: String,
    pub skip: bool,
    pub kind: FieldKind,
}

/// What we know about a field's type. Drives the codegen branch.
#[derive(Clone, Copy)]
pub enum FieldKind {
    Bool,
    SignedInt,
    UnsignedInt,
    Float,
    String,
    OptionString,
    Coord,
    /// Fall-through: assume `TryFrom<i32>` + `Into<i32>` (a `repr(i32)` enum).
    Enum,
}

pub fn parse(input: TokenStream) -> Result<RecordInput> {
    let derive: DeriveInput = parse2(input)?;
    let DeriveInput {
        ident,
        generics,
        attrs,
        data,
        ..
    } = derive;

    let (record_type, crate_path) = parse_struct_attrs(&attrs)?;
    let crate_path = crate_path.unwrap_or_else(|| parse_quote!(::altium));

    let fields = match data {
        Data::Struct(DataStruct {
            fields: Fields::Named(named),
            ..
        }) => named.named,
        _ => {
            return Err(syn::Error::new(
                ident.span(),
                "AltiumRecord can only be derived for structs with named fields",
            ));
        }
    };

    let mut parsed_fields = Vec::with_capacity(fields.len());
    for field in fields {
        parsed_fields.push(parse_field(field)?);
    }

    Ok(RecordInput {
        ident,
        generics,
        record_type,
        crate_path,
        fields: parsed_fields,
    })
}

fn parse_struct_attrs(attrs: &[Attribute]) -> Result<(Option<String>, Option<Path>)> {
    let mut record = None;
    let mut crate_path: Option<Path> = None;
    for attr in attrs {
        if !attr.path().is_ident("altium") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("record") {
                let lit: LitStr = meta.value()?.parse()?;
                record = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("crate") {
                let path: Path = meta.value()?.parse()?;
                crate_path = Some(path);
                Ok(())
            } else {
                Err(meta.error(
                    "supported struct options: `record = \"...\"`, `crate = path::to::altium`",
                ))
            }
        })?;
    }
    Ok((record, crate_path))
}

fn parse_field(field: Field) -> Result<FieldInput> {
    let span = field.span();
    let ident = field
        .ident
        .ok_or_else(|| syn::Error::new(span, "tuple struct fields are not supported"))?;
    let ty = field.ty;

    let mut param_name: Option<String> = None;
    let mut skip = false;

    for attr in &field.attrs {
        if !attr.path().is_ident("altium") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let lit: LitStr = meta.value()?.parse()?;
                param_name = Some(lit.value());
                Ok(())
            } else if meta.path.is_ident("skip") {
                skip = true;
                Ok(())
            } else if meta.path.is_ident("min_version") || meta.path.is_ident("max_version") {
                // Recognised but currently unused; consume the value so syntax
                // checks pass. Version gating is applied at the reader level.
                meta.value()?.parse::<LitStr>()?;
                Ok(())
            } else {
                Err(meta.error("unknown #[altium(...)] field option"))
            }
        })?;
    }

    let param_name = param_name.unwrap_or_else(|| ident.to_string().to_uppercase());
    let kind = classify_type(&ty);

    Ok(FieldInput {
        ident,
        ty,
        param_name,
        skip,
        kind,
    })
}

fn classify_type(ty: &Type) -> FieldKind {
    let last = match last_segment_name(ty) {
        Some(s) => s,
        None => return FieldKind::Enum,
    };
    match last.as_str() {
        "bool" => FieldKind::Bool,
        "i8" | "i16" | "i32" | "i64" | "isize" => FieldKind::SignedInt,
        "u8" | "u16" | "u32" | "u64" | "usize" => FieldKind::UnsignedInt,
        "f32" | "f64" => FieldKind::Float,
        "String" => FieldKind::String,
        "Coord" => FieldKind::Coord,
        "Option" => {
            if let Some(inner) = option_inner_type(ty) {
                if last_segment_name(inner).as_deref() == Some("String") {
                    return FieldKind::OptionString;
                }
            }
            FieldKind::Enum
        }
        _ => FieldKind::Enum,
    }
}

fn last_segment_name(ty: &Type) -> Option<String> {
    if let Type::Path(p) = ty {
        return p.path.segments.last().map(|s| s.ident.to_string());
    }
    None
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(p) = ty {
        let last = p.path.segments.last()?;
        if last.ident != "Option" {
            return None;
        }
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            for arg in &args.args {
                if let syn::GenericArgument::Type(t) = arg {
                    return Some(t);
                }
            }
        }
    }
    None
}
