use std::collections::HashSet;

use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use sqlx_core::column::Column;
use sqlx_core::describe::Describe;
use sqlx_core::type_checking::TypeChecking;
use sqlx_core::type_info::TypeInfo;
use wormx_core::{Config, TableRef, naming};

use crate::constraints::{Constraints, ForeignKey, IncomingForeignKey};
use crate::input::TableMacroInput;
use crate::mapping::rust_type_for;

struct Field {
    ident: syn::Ident,
    db_name: String,
    inner_ty: syn::Type,
    ty: syn::Type,
    sql_type: String,
    nullable: bool,
    primary_key: bool,
    has_default: bool,
    identity: bool,
    generated: bool,
    doc: String,
    attrs: Vec<TokenStream>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelationKind {
    BelongsTo,
    HasMany,
}

struct Relation {
    kind: RelationKind,
    field_ident: syn::Ident,
    field_name: String,
    storage_ty: syn::Type,
    target_ty: syn::Type,
    include_ident: syn::Ident,
    handle_ident: syn::Ident,
    fk_field: Option<syn::Ident>,
    key_ty: syn::Type,
    key_nullable: bool,
    source_key_field: Option<syn::Ident>,
    source_key_ty: Option<syn::Type>,
    target_fk_column: Option<String>,
    target_fk_accessor: Option<syn::Ident>,
    target_fk_nullable: bool,
    target_pk_accessor: Option<syn::Ident>,
    target_table: TableRef,
    source_columns: Vec<String>,
    target_columns: Vec<String>,
    /// Fields on this row that make up the relation key, in FK order.
    source_key_fields: Vec<syn::Ident>,
    /// Types of `source_key_fields`.
    source_key_tys: Vec<syn::Type>,
    /// Whether each source key field is nullable.
    source_key_nullable: Vec<bool>,
    /// Hidden accessors on the target row for the key columns.
    target_key_accessors: Vec<syn::Ident>,
    /// Whether each target key field is nullable.
    target_key_nullable: Vec<bool>,
}

impl Relation {
    fn is_composite(&self) -> bool {
        self.source_key_fields.len() > 1
    }

    fn is_optional(&self) -> bool {
        self.kind == RelationKind::BelongsTo && self.source_key_nullable.iter().any(|value| *value)
    }
}

pub fn generate<DB>(
    config: &Config,
    input: &TableMacroInput,
    describe: &Describe<DB>,
    constraints: &Constraints,
) -> syn::Result<TokenStream>
where
    DB: TypeChecking,
{
    let table = &input.table;
    let table_name = table.qualified_name();
    let struct_name = input
        .struct_name
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| {
            config
                .table_config(&table_name)
                .or_else(|| config.table_config(&table.name))
                .and_then(|table| table.struct_name.clone())
        })
        .unwrap_or_else(|| naming::struct_name(&table.name));

    let struct_ident: syn::Ident = syn::parse_str(&struct_name).map_err(|error| {
        syn::Error::new(
            input.span,
            format!("invalid struct name {struct_name:?}: {error}"),
        )
    })?;

    let mut fields = Vec::with_capacity(describe.columns().len());
    let mut field_names = HashSet::new();

    for (index, column) in describe.columns().iter().enumerate() {
        let db_name = column.name().to_owned();
        let column_options = input.columns.iter().find(|options| options.name == db_name);

        let field_ident = match column_options.and_then(|options| options.field_name.clone()) {
            Some(ident) => ident,
            None => {
                let field_name = naming::field_name(&db_name);
                syn::parse_str(&field_name).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!(
                            "invalid field name {field_name:?} for column {db_name:?}: {error}"
                        ),
                    )
                })?
            }
        };
        let field_name = field_ident.to_string();
        let field_key = field_name
            .strip_prefix("r#")
            .unwrap_or(&field_name)
            .to_owned();

        if !field_names.insert(field_key.clone()) {
            return Err(syn::Error::new(
                input.span,
                format!(
                    "table {table_name:?} has multiple columns that map to Rust field {field_key:?}; \
                     rename one of them with `as` in `table!` or with `[macros.table-overrides]`"
                ),
            ));
        }

        let nullable = describe.nullable(index).unwrap_or(true);
        let inner_ty =
            rust_type_for::<DB>(column.type_info(), config, table, &db_name, input.span)?;
        let ty: syn::Type = if nullable {
            syn::parse_quote!(::std::option::Option<#inner_ty>)
        } else {
            inner_ty.clone()
        };

        let sql_type = column.type_info().name().to_owned();
        let primary_key = constraints.is_primary_key(&db_name);
        let has_default = constraints.has_default(&db_name);
        let identity = constraints.is_identity(&db_name);
        let generated = constraints.is_generated(&db_name);

        let mut doc = format!("SQL type: `{sql_type}`");
        if nullable {
            doc.push_str(", nullable");
        } else {
            doc.push_str(", not null");
        }
        if primary_key {
            doc.push_str(", primary key");
        }
        if has_default {
            if let Some(default) = constraints.defaults.get(&db_name) {
                doc.push_str(&format!(", default `{default}`"));
            } else {
                doc.push_str(", has default");
            }
        }
        if identity {
            doc.push_str(", identity");
        }
        if generated {
            doc.push_str(", generated");
        }

        fields.push(Field {
            ident: field_ident,
            db_name,
            inner_ty,
            ty,
            sql_type,
            nullable,
            primary_key,
            has_default,
            identity,
            generated,
            doc,
            attrs: column_options
                .map(|options| options.attrs.clone())
                .unwrap_or_default(),
        });
    }

    for options in &input.columns {
        if !describe
            .columns()
            .iter()
            .any(|column| column.name() == options.name)
        {
            return Err(syn::Error::new(
                options.span,
                format!("table {table_name:?} has no column {:?}", options.name),
            ));
        }
    }

    let relations = collect_relations(
        config,
        input,
        &struct_name,
        &fields,
        &field_names,
        constraints,
    )?;

    let visibility: syn::Visibility =
        syn::parse_str(&config.codegen.visibility).map_err(|error| {
            syn::Error::new(
                input.span,
                format!(
                    "invalid visibility {:?} in wormx config: {error}",
                    config.codegen.visibility
                ),
            )
        })?;

    let derives: Vec<syn::Path> = match &input.derives {
        Some(derives) => derives.clone(),
        None => config
            .codegen
            .derives
            .iter()
            .map(|derive| {
                syn::parse_str(derive).map_err(|error| {
                    syn::Error::new(
                        input.span,
                        format!("invalid derive path {derive:?} in wormx config: {error}"),
                    )
                })
            })
            .collect::<syn::Result<_>>()?,
    };

    let derive_attribute = if derives.is_empty() {
        TokenStream::new()
    } else {
        quote! { #[derive(#(#derives),*)] }
    };
    let struct_attrs = &input.attrs;
    let relation_attrs = &input.relation_attrs;

    let struct_doc = syn::LitStr::new(
        &format!("Row type generated by `wormx::table!` for {table_name}."),
        Span::call_site(),
    );
    let field_tokens = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let doc = syn::LitStr::new(&field.doc, Span::call_site());
        let attrs = &field.attrs;
        quote! {
            #[doc = #doc]
            #(#[#attrs])*
            pub #ident: #ty
        }
    });
    let relation_field_tokens = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        let ty = &relation.storage_ty;
        let doc = format!("Generated relation `{}`.", relation.field_name);
        quote! {
            #[doc = #doc]
            #(#[#relation_attrs])*
            #ident: ::wormx::Deferred<#ty>
        }
    });

    let struct_tokens = quote! {
        #[doc = #struct_doc]
        #derive_attribute
        #(#[#struct_attrs])*
        #visibility struct #struct_ident {
            #(#field_tokens,)*
            #(#relation_field_tokens,)*
        }
    };

    let from_row_tokens = if config.codegen.from_row && !fields.is_empty() {
        quote_from_row(&struct_ident, &fields, &relations)
    } else {
        TokenStream::new()
    };

    let schema_tokens = match &table.schema {
        Some(schema) => {
            let schema = syn::LitStr::new(schema, Span::call_site());
            quote!(::std::option::Option::Some(#schema))
        }
        None => quote!(::std::option::Option::None),
    };

    let column_tokens = fields.iter().map(|field| {
        let name = syn::LitStr::new(&field.db_name, Span::call_site());
        let sql_type = syn::LitStr::new(&field.sql_type, Span::call_site());
        let nullable = field.nullable;
        let primary_key = field.primary_key;
        let has_default = field.has_default;
        let identity = field.identity;
        let generated = field.generated;
        quote! {
            ::wormx::ColumnDef {
                name: #name,
                sql_type: #sql_type,
                nullable: #nullable,
                primary_key: #primary_key,
                has_default: #has_default,
                identity: #identity,
                generated: #generated,
            }
        }
    });

    let primary_key_tokens = fields
        .iter()
        .filter(|field| field.primary_key)
        .map(|field| syn::LitStr::new(&field.db_name, Span::call_site()));

    let relation_tokens = relations.iter().map(|relation| {
        let name = syn::LitStr::new(&relation.field_name, Span::call_site());
        let kind = match relation.kind {
            RelationKind::BelongsTo => quote!(::wormx::RelationKind::BelongsTo),
            RelationKind::HasMany => quote!(::wormx::RelationKind::HasMany),
        };
        let target_table = syn::LitStr::new(&relation.target_table.name, Span::call_site());
        let target_schema = match &relation.target_table.schema {
            Some(schema) => {
                let schema = syn::LitStr::new(schema, Span::call_site());
                quote!(::std::option::Option::Some(#schema))
            }
            None => quote!(::std::option::Option::None),
        };
        let source_columns = relation
            .source_columns
            .iter()
            .map(|column| syn::LitStr::new(column, Span::call_site()));
        let target_columns = relation
            .target_columns
            .iter()
            .map(|column| syn::LitStr::new(column, Span::call_site()));
        quote! {
            ::wormx::RelationDef {
                name: #name,
                kind: #kind,
                target_table: #target_table,
                target_schema: #target_schema,
                source_columns: &[#(#source_columns),*],
                target_columns: &[#(#target_columns),*],
            }
        }
    });

    let table_name_lit = syn::LitStr::new(&table.name, Span::call_site());
    let database_url_var = syn::LitStr::new(&config.database_url_var, Span::call_site());
    let mut config_guards = TokenStream::new();
    for file in ["wormx.toml", "sqlx.toml"] {
        let path = config.manifest_dir.join(file);
        if let Some(path) = path.to_str().filter(|_| path.exists()) {
            let path = syn::LitStr::new(path, Span::call_site());
            config_guards.extend(quote! {
                const _: &'static str = ::std::include_str!(#path);
            });
        }
    }
    let env_guards = quote! {
        #config_guards
        const _: ::std::option::Option<&'static str> = ::std::option_env!(#database_url_var);
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_OFFLINE");
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_OFFLINE_DIR");
        const _: ::std::option::Option<&'static str> = ::std::option_env!("WORMX_REDO_CACHE");
    };

    let api_impl = quote_api_impl(
        &struct_ident,
        &struct_name,
        &visibility,
        &fields,
        &relations,
    )?;

    let include_impls = quote_include_impls(&struct_ident, &relations);

    let table_impl = quote! {
        impl ::wormx::Table for #struct_ident {
            const DEF: &'static ::wormx::TableDef = &::wormx::TableDef {
                name: #table_name_lit,
                schema: #schema_tokens,
                columns: &[
                    #(#column_tokens),*
                ],
                primary_key: &[
                    #(#primary_key_tokens),*
                ],
                relations: &[
                    #(#relation_tokens),*
                ],
            };
        }
    };

    Ok(quote! {
        #env_guards
        #struct_tokens
        #from_row_tokens
        #api_impl
        #include_impls
        #table_impl
    })
}

fn collect_relations(
    config: &Config,
    input: &TableMacroInput,
    struct_name: &str,
    fields: &[Field],
    column_names: &HashSet<String>,
    constraints: &Constraints,
) -> syn::Result<Vec<Relation>> {
    let mut relations = Vec::new();
    let mut used_names = column_names.clone();

    for fk in &constraints.foreign_keys {
        if fk.columns.len() != fk.referenced_columns.len() || fk.columns.is_empty() {
            continue;
        }

        let source_key_fields = fk
            .columns
            .iter()
            .filter_map(|column| fields.iter().find(|field| field.db_name == *column))
            .map(|field| field.ident.clone())
            .collect::<Vec<_>>();
        if source_key_fields.len() != fk.columns.len() {
            continue;
        }

        let source_key_tys = fk
            .columns
            .iter()
            .filter_map(|column| fields.iter().find(|field| field.db_name == *column))
            .map(|field| field.inner_ty.clone())
            .collect::<Vec<_>>();
        let source_key_nullable = fk
            .columns
            .iter()
            .filter_map(|column| fields.iter().find(|field| field.db_name == *column))
            .map(|field| field.nullable)
            .collect::<Vec<_>>();
        let optional = source_key_nullable.iter().any(|value| *value);
        let target_ty =
            relation_target_type(config, input, struct_name, &fk.referenced_table, input.span)?;
        let name = outgoing_relation_name(fk, &mut used_names);
        let storage_ty: syn::Type = if optional {
            let target = target_ty.clone();
            syn::parse_quote!(::std::option::Option<#target>)
        } else {
            target_ty.clone()
        };
        let include_ident = include_ident(struct_name, "BelongsTo", &name, input.span)?;
        let handle_ident = handle_ident(struct_name, &name, input.span)?;
        let target_key_accessors = fk
            .referenced_columns
            .iter()
            .map(|column| column_accessor_ident(column))
            .collect::<Vec<_>>();
        let target_pk_accessor = target_key_accessors.first().cloned();
        let key_ty = if source_key_tys.len() == 1 {
            source_key_tys[0].clone()
        } else {
            syn::parse_quote!((#(#source_key_tys),*))
        };

        relations.push(Relation {
            kind: RelationKind::BelongsTo,
            field_ident: relation_ident(&name, input.span)?,
            field_name: name,
            storage_ty,
            target_ty,
            include_ident,
            handle_ident,
            fk_field: source_key_fields.first().cloned(),
            key_ty,
            key_nullable: optional,
            source_key_field: None,
            source_key_ty: None,
            target_fk_column: None,
            target_fk_accessor: None,
            target_fk_nullable: false,
            target_pk_accessor,
            target_table: fk.referenced_table.clone(),
            source_columns: fk.columns.clone(),
            target_columns: fk.referenced_columns.clone(),
            source_key_fields,
            source_key_tys,
            source_key_nullable,
            target_key_accessors,
            target_key_nullable: vec![false; fk.referenced_columns.len()],
        });
    }

    let mut incoming_counts = std::collections::HashMap::new();
    for incoming in &constraints.referenced_by {
        *incoming_counts
            .entry(incoming.source_table.qualified_name())
            .or_insert(0usize) += 1;
    }

    for incoming in &constraints.referenced_by {
        if incoming.columns.len() != incoming.referenced_columns.len()
            || incoming.columns.is_empty()
        {
            continue;
        }

        let source_fields = incoming
            .referenced_columns
            .iter()
            .filter_map(|column| fields.iter().find(|field| field.db_name == *column))
            .collect::<Vec<_>>();
        if source_fields.len() != incoming.referenced_columns.len() {
            continue;
        }
        if source_fields.iter().any(|field| field.nullable) {
            continue;
        }

        let source_key_tys = source_fields
            .iter()
            .map(|field| field.inner_ty.clone())
            .collect::<Vec<_>>();
        let target_ty = relation_target_type(
            config,
            input,
            struct_name,
            &incoming.source_table,
            input.span,
        )?;
        let count = incoming_counts
            .get(&incoming.source_table.qualified_name())
            .copied()
            .unwrap_or(1);
        let name = incoming_relation_name(incoming, count, &mut used_names);
        let include_ident = include_ident(struct_name, "HasMany", &name, input.span)?;
        let handle_ident = handle_ident(struct_name, &name, input.span)?;
        let target_key_accessors = incoming
            .columns
            .iter()
            .map(|column| column_accessor_ident(column))
            .collect::<Vec<_>>();
        let key_ty = if source_key_tys.len() == 1 {
            source_key_tys[0].clone()
        } else {
            syn::parse_quote!((#(#source_key_tys),*))
        };

        relations.push(Relation {
            kind: RelationKind::HasMany,
            field_ident: relation_ident(&name, input.span)?,
            field_name: name,
            storage_ty: syn::parse_quote!(::std::vec::Vec<#target_ty>),
            target_ty,
            include_ident,
            handle_ident,
            fk_field: None,
            key_ty,
            key_nullable: false,
            source_key_field: Some(source_fields[0].ident.clone()),
            source_key_ty: Some(source_key_tys[0].clone()),
            target_fk_column: Some(incoming.columns[0].clone()),
            target_fk_accessor: target_key_accessors.first().cloned(),
            target_fk_nullable: incoming.columns_nullable.iter().any(|nullable| *nullable),
            target_pk_accessor: None,
            target_table: incoming.source_table.clone(),
            source_columns: incoming.referenced_columns.clone(),
            target_columns: incoming.columns.clone(),
            source_key_fields: source_fields
                .iter()
                .map(|field| field.ident.clone())
                .collect(),
            source_key_tys,
            source_key_nullable: source_fields.iter().map(|field| field.nullable).collect(),
            target_key_accessors,
            target_key_nullable: incoming.columns_nullable.clone(),
        });
    }

    Ok(relations)
}

fn outgoing_relation_name(fk: &ForeignKey, used: &mut HashSet<String>) -> String {
    let base = if fk.columns.len() > 1 {
        fk.referenced_table.name.clone()
    } else {
        fk.columns
            .first()
            .and_then(|column| column.strip_suffix("_id"))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| fk.referenced_table.name.clone())
    };

    unique_name(&naming::field_name(&base), used)
}

fn incoming_relation_name(
    incoming: &IncomingForeignKey,
    count: usize,
    used: &mut HashSet<String>,
) -> String {
    let base = if count > 1 {
        let fk = incoming
            .columns
            .first()
            .map(|column| column.strip_suffix("_id").unwrap_or(column))
            .filter(|column| !column.is_empty())
            .unwrap_or("source");
        format!("{fk}_{}", incoming.source_table.name)
    } else {
        incoming.source_table.name.clone()
    };

    unique_name(&naming::field_name(&base), used)
}

fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_owned()) {
        return base.to_owned();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn relation_ident(name: &str, span: Span) -> syn::Result<syn::Ident> {
    syn::parse_str(name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid relation field name {name:?}: {error}"),
        )
    })
}

/// A hidden accessor for a column, keyed by its database name.
///
/// Relation code is generated in a different `table!` invocation than the
/// target row type, so it cannot know whether the target column's field was
/// renamed with `as`. Encoding the database name keeps both sides in agreement:
/// the target generates the accessor, and the source calls it by column name.
fn column_accessor_ident(column: &str) -> syn::Ident {
    syn::Ident::new(
        &format!("__wormx_column_{}", hex::encode(column.as_bytes())),
        Span::call_site(),
    )
}

fn include_ident(struct_name: &str, kind: &str, name: &str, span: Span) -> syn::Result<syn::Ident> {
    let name = format!(
        "{}{}{}Include",
        struct_name.trim_start_matches("r#"),
        kind,
        name.trim_start_matches("r#").to_upper_camel_case()
    );
    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(span, format!("invalid include type name {name:?}: {error}"))
    })
}

fn handle_ident(struct_name: &str, name: &str, span: Span) -> syn::Result<syn::Ident> {
    let name = format!(
        "{}Relation{}",
        struct_name.trim_start_matches("r#"),
        name.trim_start_matches("r#").to_upper_camel_case()
    );
    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid relation type name {name:?}: {error}"),
        )
    })
}

fn relation_target_type(
    config: &Config,
    input: &TableMacroInput,
    struct_name: &str,
    table: &TableRef,
    span: Span,
) -> syn::Result<syn::Type> {
    if table == &input.table {
        return syn::parse_str(struct_name).map_err(|error| {
            syn::Error::new(
                span,
                format!("invalid self-relation struct name {struct_name:?}: {error}"),
            )
        });
    }

    target_type(config, table, span)
}

fn target_type(config: &Config, table: &TableRef, span: Span) -> syn::Result<syn::Type> {
    let table_config = config
        .table_config(&table.qualified_name())
        .or_else(|| config.table_config(&table.name));

    if let Some(path) = table_config.and_then(|config| config.rust_path.as_deref()) {
        return syn::parse_str(path).map_err(|error| {
            syn::Error::new(
                span,
                format!("invalid rust-path {path:?} for table {table}: {error}"),
            )
        });
    }

    let name = table_config
        .and_then(|config| config.struct_name.clone())
        .unwrap_or_else(|| naming::struct_name(&table.name));

    syn::parse_str(&name).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid target struct name {name:?} for table {table}: {error}"),
        )
    })
}

fn quote_api_impl(
    struct_ident: &syn::Ident,
    struct_name: &str,
    visibility: &syn::Visibility,
    fields: &[Field],
    relations: &[Relation],
) -> syn::Result<TokenStream> {
    let fields_ident: syn::Ident =
        syn::parse_str(&format!("{}Fields", struct_name.trim_start_matches("r#")))?;

    let field_handles = fields.iter().map(|field| {
        let ident = &field.ident;
        let inner_ty = &field.inner_ty;
        let db_name = syn::LitStr::new(&field.db_name, Span::call_site());
        quote! {
            pub fn #ident(&self) -> ::wormx::Column<#inner_ty> {
                ::wormx::Column::with_alias(
                    <#struct_ident as ::wormx::Table>::DEF.schema,
                    <#struct_ident as ::wormx::Table>::DEF.name,
                    self.alias,
                    #db_name,
                )
            }
        }
    });

    let column_accessors = fields.iter().map(|field| {
        let accessor = column_accessor_ident(&field.db_name);
        let ident = &field.ident;
        let ty = &field.ty;
        quote! {
            #[doc(hidden)]
            pub fn #accessor(&self) -> &#ty {
                &self.#ident
            }
        }
    });

    let relation_handles = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        let include = &relation.include_ident;
        quote! {
            pub fn #ident<D>(&self) -> #include<D>
            where
                D: ::wormx::Dialect + ::std::clone::Clone,
            {
                #include::new()
            }
        }
    });

    let constructor_params = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote!(#ident: #ty)
    });
    let constructor_fields = fields.iter().map(|field| {
        let ident = &field.ident;
        quote!(#ident)
    });
    let constructor_relations = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        quote!(#ident: ::wormx::Deferred::unloaded())
    });

    let primary_key_field = if fields.iter().filter(|field| field.primary_key).count() == 1 {
        fields.iter().find(|field| field.primary_key).map(|field| {
            let inner_ty = &field.inner_ty;
            quote! {
                pub fn get(key: #inner_ty) -> ::wormx::Get<#struct_ident, #inner_ty> {
                    ::wormx::Get::new(key)
                }
            }
        })
    } else {
        None
    };

    let relation_accessors = relations.iter().map(|relation| {
        let field_ident = &relation.field_ident;
        let target_ty = &relation.target_ty;
        let field_name = &relation.field_name;

        if relation.is_composite() {
            let handle = &relation.handle_ident;
            let keys = relation
                .source_key_fields
                .iter()
                .map(|field| quote!(self.#field.clone()));
            return quote! {
                #[doc = concat!("Access the `", #field_name, "` relation.")]
                pub fn #field_ident(&self) -> #handle<'_> {
                    #handle::new(#(#keys,)* &self.#field_ident)
                }
            };
        }

        match relation.kind {
            RelationKind::BelongsTo => {
                let fk_field = relation.fk_field.as_ref().unwrap();
                let key_ty = &relation.key_ty;
                let target_column = syn::LitStr::new(
                    relation
                        .target_columns
                        .first()
                        .expect("belongs-to relation has a referenced column"),
                    Span::call_site(),
                );
                let (return_ty, constructor) = if relation.key_nullable {
                    (
                        quote!(::wormx::OptionalBelongsToRef<'_, #target_ty, #key_ty>),
                        quote! {
                            ::wormx::OptionalBelongsToRef::with_column(
                                self.#fk_field.clone(),
                                #target_column,
                                &self.#field_ident,
                            )
                        },
                    )
                } else {
                    (
                        quote!(::wormx::BelongsToRef<'_, #target_ty, #key_ty>),
                        quote! {
                            ::wormx::BelongsToRef::with_column(
                                self.#fk_field.clone(),
                                #target_column,
                                &self.#field_ident,
                            )
                        },
                    )
                };

                quote! {
                    #[doc = concat!("Access the `", #field_name, "` relation.")]
                    pub fn #field_ident(&self) -> #return_ty {
                        #constructor
                    }
                }
            }
            RelationKind::HasMany => {
                let source_key_field = relation.source_key_field.as_ref().unwrap();
                let source_key_ty = relation.source_key_ty.as_ref().unwrap();
                let target_fk_column = syn::LitStr::new(
                    relation.target_fk_column.as_deref().unwrap_or(""),
                    Span::call_site(),
                );

                quote! {
                    #[doc = concat!("Access the `", #field_name, "` relation.")]
                    pub fn #field_ident(
                        &self,
                    ) -> ::wormx::HasManyRef<'_, #target_ty, #source_key_ty> {
                        ::wormx::HasManyRef::new(
                            self.#source_key_field.clone(),
                            #target_fk_column,
                            &self.#field_ident,
                        )
                    }
                }
            }
        }
    });

    let composite_handles = relations
        .iter()
        .filter(|relation| relation.is_composite())
        .map(quote_composite_handle);

    Ok(quote! {
        #(#composite_handles)*

        #[derive(Debug, Clone, Copy, Default)]
        #visibility struct #fields_ident {
            alias: ::std::option::Option<&'static str>,
        }

        impl #fields_ident {
            #(#field_handles)*
            #(#relation_handles)*

            /// The table source for a join.
            pub fn table(&self) -> ::wormx::TableSource {
                ::wormx::TableSource::new(
                    <#struct_ident as ::wormx::Table>::DEF,
                    self.alias,
                )
            }
        }

        impl #struct_ident {
            #(#column_accessors)*

            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#constructor_params),*) -> Self {
                Self {
                    #(#constructor_fields,)*
                    #(#constructor_relations,)*
                }
            }

            pub fn all() -> ::wormx::Query<#struct_ident> {
                ::wormx::Query::new()
            }

            #primary_key_field

            pub fn fields() -> #fields_ident {
                #fields_ident { alias: ::std::option::Option::None }
            }

            /// Column handles qualified by an alias, for joins and self-joins.
            pub fn alias(alias: &'static str) -> #fields_ident {
                #fields_ident {
                    alias: ::std::option::Option::Some(alias),
                }
            }

            #(#relation_accessors)*
        }
    })
}

fn quote_composite_handle(relation: &Relation) -> TokenStream {
    let handle = &relation.handle_ident;
    let target_ty = &relation.target_ty;
    let field_name = &relation.field_name;
    let storage_ty = &relation.storage_ty;
    let optional = relation.is_optional();

    let key_params = relation
        .source_key_fields
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let ident = relation_key_ident(index);
            let ty = &relation.source_key_tys[index];
            if relation.source_key_nullable[index] {
                quote!(#ident: ::std::option::Option<#ty>)
            } else {
                quote!(#ident: #ty)
            }
        })
        .collect::<Vec<_>>();
    let key_idents = relation
        .source_key_fields
        .iter()
        .enumerate()
        .map(|(index, _)| relation_key_ident(index))
        .collect::<Vec<_>>();

    let loaded = match relation.kind {
        RelationKind::BelongsTo if optional => quote! {
            pub fn loaded(self) -> ::std::option::Option<&'a #target_ty> {
                self.deferred.get().and_then(::std::option::Option::as_ref)
            }

            pub fn expect_loaded(self) -> &'a #target_ty {
                self.loaded().expect(
                    "relation is not loaded or is null; use `.include(...)` first",
                )
            }
        },
        RelationKind::BelongsTo => quote! {
            pub fn loaded(self) -> ::std::option::Option<&'a #target_ty> {
                self.deferred.get()
            }

            pub fn expect_loaded(self) -> &'a #target_ty {
                self.deferred.expect_loaded()
            }
        },
        RelationKind::HasMany => quote! {
            pub fn loaded(self) -> ::std::option::Option<&'a ::std::vec::Vec<#target_ty>> {
                self.deferred.get()
            }

            pub fn expect_loaded(self) -> &'a ::std::vec::Vec<#target_ty> {
                self.deferred.expect_loaded()
            }
        },
    };

    let filters = relation
        .source_columns
        .iter()
        .zip(relation.target_columns.iter())
        .enumerate()
        .map(|(index, (source_column, target_column))| {
            let _ = source_column;
            let key = relation_key_ident(index);
            let ty = &relation.source_key_tys[index];
            let column = syn::LitStr::new(target_column, Span::call_site());
            let value = if relation.source_key_nullable[index] {
                quote!(self.#key.clone().unwrap())
            } else {
                quote!(self.#key.clone())
            };
            quote! {
                select = select.filter(
                    ::wormx::Column::<#ty>::new(
                        <#target_ty as ::wormx::Table>::DEF.schema,
                        <#target_ty as ::wormx::Table>::DEF.name,
                        #column,
                    )
                    .eq::<D, _>(#value),
                );
            }
        });

    let nullable_check = if optional {
        let checks = relation
            .source_key_fields
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let key = relation_key_ident(index);
                quote!(self.#key.is_none())
            });
        quote! {
            if #(#checks)||* {
                return select.filter(::wormx::Expr::<bool, D>::literal(false));
            }
        }
    } else {
        TokenStream::new()
    };

    let key_bounds = relation
        .source_key_tys
        .iter()
        .map(|ty| {
            quote! {
                #ty: ::std::clone::Clone
                    + ::std::marker::Send
                    + 'static
                    + for<'q> ::wormx::sqlx::Encode<'q, D::DB>
                    + ::wormx::sqlx::Type<D::DB>
            }
        })
        .collect::<Vec<_>>();

    quote! {
        #[doc = concat!("Composite relation handle for `", #field_name, "`.")]
        pub struct #handle<'a> {
            #(#key_params,)*
            deferred: &'a ::wormx::Deferred<#storage_ty>,
        }

        impl<'a> #handle<'a> {
            #[allow(clippy::too_many_arguments)]
            pub const fn new(
                #(#key_params,)*
                deferred: &'a ::wormx::Deferred<#storage_ty>,
            ) -> Self {
                Self {
                    #(#key_idents,)*
                    deferred,
                }
            }

            #loaded

            pub fn is_loaded(&self) -> bool {
                self.deferred.is_loaded()
            }
        }

        impl<D> ::wormx::IntoSelect<D> for #handle<'_>
        where
            D: ::wormx::Dialect + ::std::clone::Clone,
            #(#key_bounds,)*
        {
            type Table = #target_ty;

            fn into_select(self, db: &::wormx::Db<D>) -> ::wormx::Select<D, #target_ty> {
                let mut select = db.query(<#target_ty>::all());
                #nullable_check
                #(#filters)*
                select
            }
        }
    }
}

fn relation_key_ident(index: usize) -> syn::Ident {
    syn::Ident::new(&format!("key_{index}"), Span::call_site())
}

fn quote_include_impls(struct_ident: &syn::Ident, relations: &[Relation]) -> TokenStream {
    let impls = relations.iter().map(|relation| {
        let include_ident = &relation.include_ident;
        let target_ty = &relation.target_ty;
        let key_ty = &relation.key_ty;
        let key_ty_bound = if relation.is_composite() {
            quote! {
                #key_ty: ::std::clone::Clone
                    + ::std::cmp::Eq
                    + ::std::hash::Hash
                    + 'static,
            }
        } else {
            quote! {
                #key_ty: ::std::clone::Clone
                    + ::std::cmp::Eq
                    + ::std::hash::Hash
                    + 'static
                    + for<'q> ::wormx::sqlx::Encode<'q, D::DB>
                    + ::wormx::sqlx::Type<D::DB>,
            }
        };
        let component_bounds = if relation.is_composite() {
            let bounds = relation.source_key_tys.iter().map(|ty| {
                quote! {
                    #ty: ::std::clone::Clone
                        + ::std::marker::Send
                        + 'static
                        + for<'q> ::wormx::sqlx::Encode<'q, D::DB>
                        + ::wormx::sqlx::Type<D::DB>,
                }
            });
            quote!(#(#bounds)*)
        } else {
            TokenStream::new()
        };

        let pool_body = include_body(relation, false);
        let tx_body = include_body(relation, true);

        let target_bound = quote!(#target_ty: ::wormx::Table + ::std::clone::Clone);

        quote! {
            pub struct #include_ident<D: ::wormx::Dialect + ::std::clone::Clone> {
                nested: ::std::vec::Vec<
                    ::std::boxed::Box<
                        dyn ::wormx::Include<D, #target_ty>,
                    >,
                >,
            }

            impl<D: ::wormx::Dialect + ::std::clone::Clone> #include_ident<D> {
                pub fn new() -> Self {
                    Self {
                        nested: ::std::vec::Vec::new(),
                    }
                }

                /// Adds an eager-loading step to apply to the related rows.
                pub fn include<I>(mut self, include: I) -> Self
                where
                    I: ::wormx::Include<D, #target_ty> + 'static,
                {
                    self.nested.push(::std::boxed::Box::new(include));
                    self
                }
            }

            impl<D: ::wormx::Dialect + ::std::clone::Clone> ::std::default::Default
                for #include_ident<D>
            {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<D> ::wormx::Include<D, #struct_ident> for #include_ident<D>
            where
                D: ::wormx::Dialect + ::std::clone::Clone + ::std::marker::Send + ::std::marker::Sync,
                #struct_ident: ::std::marker::Send + ::std::marker::Sync,
                #target_bound
                    + ::std::marker::Send
                    + ::std::marker::Sync
                    + ::std::marker::Unpin
                    + for<'r> ::wormx::sqlx::FromRow<
                        'r,
                        <D::DB as ::wormx::sqlx::Database>::Row,
                    >,
                #key_ty_bound
                #component_bounds
                <D::DB as ::wormx::sqlx::Database>::Arguments:
                    ::wormx::sqlx::IntoArguments<D::DB>,
                for<'c> &'c mut <D::DB as ::wormx::sqlx::Database>::Connection:
                    ::wormx::sqlx::Executor<'c, Database = D::DB>,
            {
                fn apply<'a>(
                    &'a self,
                    db: &'a ::wormx::Db<D>,
                    rows: &'a mut [#struct_ident],
                ) -> ::std::pin::Pin<
                    ::std::boxed::Box<
                        dyn ::std::future::Future<
                                Output = ::wormx::sqlx::Result<()>,
                            > + ::std::marker::Send
                            + 'a,
                    >,
                > {
                    let nested = &self.nested;
                    ::std::boxed::Box::pin(async move {
                        #pool_body
                    })
                }

                fn apply_in<'a>(
                    &'a self,
                    db: &'a ::wormx::Db<D>,
                    tx: &'a mut ::wormx::Transaction<'_, D>,
                    rows: &'a mut [#struct_ident],
                ) -> ::std::pin::Pin<
                    ::std::boxed::Box<
                        dyn ::std::future::Future<
                                Output = ::wormx::sqlx::Result<()>,
                            > + ::std::marker::Send
                            + 'a,
                    >,
                > {
                    let nested = &self.nested;
                    ::std::boxed::Box::pin(async move {
                        #tx_body
                    })
                }
            }
        }
    });

    quote! {
        #(#impls)*
    }
}

fn include_body(relation: &Relation, in_transaction: bool) -> TokenStream {
    if relation.is_composite() {
        return include_body_composite(relation, in_transaction);
    }

    let target_ty = &relation.target_ty;
    let key_ty = &relation.key_ty;
    let exec = if in_transaction {
        quote!(exec_in(tx))
    } else {
        quote!(exec())
    };
    let apply = if in_transaction {
        quote!(apply_in(db, tx, &mut related))
    } else {
        quote!(apply(db, &mut related))
    };

    match relation.kind {
        RelationKind::BelongsTo => {
            let fk_field = relation.fk_field.as_ref().unwrap();
            let target_pk_accessor = relation.target_pk_accessor.as_ref().unwrap();
            let target_column = syn::LitStr::new(
                relation
                    .target_columns
                    .first()
                    .expect("belongs-to relation has a referenced column"),
                Span::call_site(),
            );
            let field_ident = &relation.field_ident;
            let keys = if relation.key_nullable {
                quote! {
                    rows.iter()
                        .filter_map(|row| row.#fk_field.clone())
                        .collect::<::std::vec::Vec<#key_ty>>()
                }
            } else {
                quote! {
                    rows.iter()
                        .map(|row| row.#fk_field.clone())
                        .collect::<::std::vec::Vec<#key_ty>>()
                }
            };
            let set = if relation.key_nullable {
                quote! {
                    row.#field_ident.set(
                        row.#fk_field
                            .clone()
                            .and_then(|key| by_key.get(&key).cloned()),
                    );
                }
            } else {
                quote! {
                    if let ::std::option::Option::Some(target) =
                        by_key.get(&row.#fk_field).cloned()
                    {
                        row.#field_ident.set(target);
                    }
                }
            };
            let empty = if relation.key_nullable {
                quote! {
                    for row in rows {
                        row.#field_ident.set(::std::option::Option::None);
                    }
                    return ::std::result::Result::Ok(());
                }
            } else {
                quote! {
                    return ::std::result::Result::Ok(());
                }
            };

            quote! {
                let keys = #keys;
                if keys.is_empty() {
                    #empty
                }

                let keys = keys
                    .into_iter()
                    .collect::<::std::collections::HashSet<#key_ty>>()
                    .into_iter()
                    .collect::<::std::vec::Vec<#key_ty>>();
                let mut related: ::std::vec::Vec<#target_ty> = ::std::vec::Vec::new();
                for chunk in keys.chunks(900) {
                    related.extend(
                        db
                            .query(<#target_ty>::all())
                            .filter(
                                ::wormx::Column::<#key_ty>::new(
                                    <#target_ty as ::wormx::Table>::DEF.schema,
                                    <#target_ty as ::wormx::Table>::DEF.name,
                                    #target_column,
                                )
                                .in_list(chunk.to_vec()),
                            )
                            .#exec
                            .await?,
                    );
                }

                for nested_include in nested {
                    nested_include.#apply.await?;
                }

                let mut by_key: ::std::collections::HashMap<#key_ty, #target_ty> = related
                    .into_iter()
                    .map(|row| (row.#target_pk_accessor().clone(), row))
                    .collect();

                for row in rows {
                    #set
                }

                ::std::result::Result::Ok(())
            }
        }
        RelationKind::HasMany => {
            let source_key_field = relation.source_key_field.as_ref().unwrap();
            let source_key_ty = relation.source_key_ty.as_ref().unwrap();
            let target_fk_accessor = relation.target_fk_accessor.as_ref().unwrap();
            let target_fk_column = syn::LitStr::new(
                relation
                    .target_fk_column
                    .as_deref()
                    .expect("has-many relation has a foreign-key column"),
                Span::call_site(),
            );
            let field_ident = &relation.field_ident;
            let group = if relation.target_fk_nullable {
                quote! {
                    for target in related {
                        if let ::std::option::Option::Some(key) =
                            target.#target_fk_accessor().clone()
                        {
                            grouped.entry(key).or_default().push(target);
                        }
                    }
                }
            } else {
                quote! {
                    for target in related {
                        grouped
                            .entry(target.#target_fk_accessor().clone())
                            .or_default()
                            .push(target);
                    }
                }
            };

            quote! {
                let keys = rows
                    .iter()
                    .map(|row| row.#source_key_field.clone())
                    .collect::<::std::vec::Vec<#source_key_ty>>();
                if keys.is_empty() {
                    return ::std::result::Result::Ok(());
                }

                let keys = keys
                    .into_iter()
                    .collect::<::std::collections::HashSet<#source_key_ty>>()
                    .into_iter()
                    .collect::<::std::vec::Vec<#source_key_ty>>();
                let mut related: ::std::vec::Vec<#target_ty> = ::std::vec::Vec::new();
                for chunk in keys.chunks(900) {
                    related.extend(
                        db
                            .query(<#target_ty>::all())
                            .filter(
                                ::wormx::Column::<#source_key_ty>::new(
                                    <#target_ty as ::wormx::Table>::DEF.schema,
                                    <#target_ty as ::wormx::Table>::DEF.name,
                                    #target_fk_column,
                                )
                                .in_list(chunk.to_vec()),
                            )
                            .#exec
                            .await?,
                    );
                }

                for nested_include in nested {
                    nested_include.#apply.await?;
                }

                let mut grouped: ::std::collections::HashMap<
                    #source_key_ty,
                    ::std::vec::Vec<#target_ty>,
                > = ::std::collections::HashMap::new();
                #group

                for row in rows {
                    row.#field_ident.set(
                        grouped
                            .get(&row.#source_key_field)
                            .cloned()
                            .unwrap_or_default(),
                    );
                }

                ::std::result::Result::Ok(())
            }
        }
    }
}

fn include_body_composite(relation: &Relation, in_transaction: bool) -> TokenStream {
    let target_ty = &relation.target_ty;
    let exec = if in_transaction {
        quote!(exec_in(tx))
    } else {
        quote!(exec())
    };
    let apply = if in_transaction {
        quote!(apply_in(db, tx, &mut related))
    } else {
        quote!(apply(db, &mut related))
    };
    let key_tys = relation.source_key_tys.clone();
    let key_len = key_tys.len();
    let key_ty: syn::Type = syn::parse_quote!((#(#key_tys),*));

    let filter = {
        let expressions = relation
            .target_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let ty = &relation.source_key_tys[index];
                let value = tuple_index_tokens(&quote!(key), index);
                let column = syn::LitStr::new(column, Span::call_site());
                quote! {
                    ::wormx::Column::<#ty>::new(
                        <#target_ty as ::wormx::Table>::DEF.schema,
                        <#target_ty as ::wormx::Table>::DEF.name,
                        #column,
                    )
                    .eq::<D, _>(#value)
                }
            })
            .collect::<Vec<_>>();
        let first = &expressions[0];
        let rest = &expressions[1..];
        quote! {
            let mut __filter: ::std::option::Option<
                ::wormx::Expr<bool, D>,
            > = ::std::option::Option::None;
            for key in chunk.iter().cloned() {
                let __current = #first #(.and(#rest))*;
                __filter = ::std::option::Option::Some(match __filter {
                    ::std::option::Option::Some(existing) => existing.or(__current),
                    ::std::option::Option::None => __current,
                });
            }
        }
    };

    match relation.kind {
        RelationKind::BelongsTo => {
            let optional = relation.is_optional();
            let keys = {
                if optional {
                    let patterns = relation
                        .source_key_fields
                        .iter()
                        .enumerate()
                        .map(|(index, _)| {
                            let value = relation_key_ident(index);
                            quote!(::std::option::Option::Some(#value))
                        })
                        .collect::<Vec<_>>();
                    let row_values = relation
                        .source_key_fields
                        .iter()
                        .map(|field| quote!(row.#field.clone()))
                        .collect::<Vec<_>>();
                    let values = relation
                        .source_key_fields
                        .iter()
                        .enumerate()
                        .map(|(index, _)| relation_key_ident(index))
                        .collect::<Vec<_>>();
                    quote! {
                        rows.iter()
                            .filter_map(|row| match (#(#row_values),*) {
                                (#(#patterns),*) => ::std::option::Option::Some((#(#values),*)),
                                _ => ::std::option::Option::None,
                            })
                            .collect::<::std::vec::Vec<#key_ty>>()
                    }
                } else {
                    let values = relation
                        .source_key_fields
                        .iter()
                        .map(|field| quote!(row.#field.clone()))
                        .collect::<Vec<_>>();
                    quote! {
                        rows.iter()
                            .map(|row| (#(#values),*))
                            .collect::<::std::vec::Vec<#key_ty>>()
                    }
                }
            };
            let empty = if optional {
                let field = &relation.field_ident;
                quote! {
                    for row in rows {
                        row.#field.set(::std::option::Option::None);
                    }
                    return ::std::result::Result::Ok(());
                }
            } else {
                quote! {
                    return ::std::result::Result::Ok(());
                }
            };
            let map_key = {
                let values = relation
                    .target_key_accessors
                    .iter()
                    .map(|accessor| quote!(row.#accessor().clone()));
                quote!((#(#values),*))
            };
            let set = if optional {
                let field = &relation.field_ident;
                let patterns = relation
                    .source_key_fields
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        let value = relation_key_ident(index);
                        quote!(::std::option::Option::Some(#value))
                    })
                    .collect::<Vec<_>>();
                let row_values = relation
                    .source_key_fields
                    .iter()
                    .map(|field| quote!(row.#field.clone()))
                    .collect::<Vec<_>>();
                let values = relation
                    .source_key_fields
                    .iter()
                    .enumerate()
                    .map(|(index, _)| relation_key_ident(index))
                    .collect::<Vec<_>>();
                quote! {
                    if let (#(#patterns),*) = (#(#row_values),*) {
                        row.#field.set(by_key.get(&(#(#values),*)).cloned());
                    } else {
                        row.#field.set(::std::option::Option::None);
                    }
                }
            } else {
                let field = &relation.field_ident;
                let values = relation
                    .source_key_fields
                    .iter()
                    .map(|field| quote!(row.#field.clone()));
                quote! {
                    if let ::std::option::Option::Some(target) =
                        by_key.get(&(#(#values),*)).cloned()
                    {
                        row.#field.set(target);
                    }
                }
            };

            quote! {
                let keys = #keys;
                if keys.is_empty() {
                    #empty
                }

                let keys = keys
                    .into_iter()
                    .collect::<::std::collections::HashSet<#key_ty>>()
                    .into_iter()
                    .collect::<::std::vec::Vec<#key_ty>>();
                let chunk_size = (900usize / #key_len).max(1);
                let mut related: ::std::vec::Vec<#target_ty> = ::std::vec::Vec::new();
                for chunk in keys.chunks(chunk_size) {
                    #filter
                    related.extend(
                        db
                            .query(<#target_ty>::all())
                            .filter(__filter.unwrap())
                            .#exec
                            .await?,
                    );
                }

                for nested_include in nested {
                    nested_include.#apply.await?;
                }

                let mut by_key: ::std::collections::HashMap<#key_ty, #target_ty> = related
                    .into_iter()
                    .map(|row| (#map_key, row))
                    .collect();

                for row in rows {
                    #set
                }

                ::std::result::Result::Ok(())
            }
        }
        RelationKind::HasMany => {
            let field = &relation.field_ident;
            let keys = {
                let values = relation
                    .source_key_fields
                    .iter()
                    .map(|field| quote!(row.#field.clone()));
                quote! {
                    rows.iter()
                        .map(|row| (#(#values),*))
                        .collect::<::std::vec::Vec<#key_ty>>()
                }
            };
            let group_key = {
                let values = relation
                    .target_key_accessors
                    .iter()
                    .map(|accessor| quote!(target.#accessor().clone()));
                quote!((#(#values),*))
            };
            let group_statement = if relation.target_key_nullable.iter().any(|value| *value) {
                let patterns = relation
                    .target_key_accessors
                    .iter()
                    .enumerate()
                    .map(|(index, _)| {
                        let value = relation_key_ident(index);
                        if relation.target_key_nullable[index] {
                            quote!(::std::option::Option::Some(#value))
                        } else {
                            quote!(#value)
                        }
                    })
                    .collect::<Vec<_>>();
                let row_values = relation
                    .target_key_accessors
                    .iter()
                    .map(|accessor| quote!(target.#accessor().clone()))
                    .collect::<Vec<_>>();
                let values = relation
                    .target_key_accessors
                    .iter()
                    .enumerate()
                    .map(|(index, _)| relation_key_ident(index))
                    .collect::<Vec<_>>();
                quote! {
                    for target in related {
                        if let (#(#patterns),*) = (#(#row_values),*) {
                            grouped
                                .entry((#(#values),*))
                                .or_default()
                                .push(target);
                        }
                    }
                }
            } else {
                quote! {
                    for target in related {
                        grouped.entry(#group_key).or_default().push(target);
                    }
                }
            };
            let remove_key = {
                let values = relation
                    .source_key_fields
                    .iter()
                    .map(|field| quote!(row.#field.clone()));
                quote!((#(#values),*))
            };

            quote! {
                let keys = #keys;
                if keys.is_empty() {
                    return ::std::result::Result::Ok(());
                }

                let keys = keys
                    .into_iter()
                    .collect::<::std::collections::HashSet<#key_ty>>()
                    .into_iter()
                    .collect::<::std::vec::Vec<#key_ty>>();
                let chunk_size = (900usize / #key_len).max(1);
                let mut related: ::std::vec::Vec<#target_ty> = ::std::vec::Vec::new();
                for chunk in keys.chunks(chunk_size) {
                    #filter
                    related.extend(
                        db
                            .query(<#target_ty>::all())
                            .filter(__filter.unwrap())
                            .#exec
                            .await?,
                    );
                }

                for nested_include in nested {
                    nested_include.#apply.await?;
                }

                let mut grouped: ::std::collections::HashMap<
                    #key_ty,
                    ::std::vec::Vec<#target_ty>,
                > = ::std::collections::HashMap::new();
                #group_statement

                for row in rows {
                    row.#field.set(
                        grouped
                            .get(&#remove_key)
                            .cloned()
                            .unwrap_or_default(),
                    );
                }

                ::std::result::Result::Ok(())
            }
        }
    }
}

fn tuple_index_tokens(tuple: &TokenStream, index: usize) -> TokenStream {
    let index = syn::Index::from(index);
    quote!(#tuple.#index)
}

fn quote_from_row(
    struct_ident: &syn::Ident,
    fields: &[Field],
    relations: &[Relation],
) -> TokenStream {
    let mut seen = HashSet::new();
    let bounds = fields.iter().filter_map(|field| {
        let ty = &field.ty;
        let key = ty.to_token_stream().to_string();
        if !seen.insert(key) {
            return None;
        }
        Some(quote! {
            #ty: ::wormx::__private::Decode<
                    'r,
                    <__WormxRow as ::wormx::__private::Row>::Database
                >
                + ::wormx::__private::Type<
                    <__WormxRow as ::wormx::__private::Row>::Database
                >
        })
    });

    let init = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let db_name = syn::LitStr::new(&field.db_name, Span::call_site());
        quote! {
            #ident: ::wormx::__private::Row::try_get::<#ty, _>(__row, #db_name)?
        }
    });
    let relation_init = relations.iter().map(|relation| {
        let ident = &relation.field_ident;
        quote!(#ident: ::wormx::Deferred::unloaded())
    });

    quote! {
        impl<'r, __WormxRow> ::wormx::__private::FromRow<'r, __WormxRow> for #struct_ident
        where
            __WormxRow: ::wormx::__private::Row,
            for<'index> &'index str:
                ::wormx::__private::ColumnIndex<__WormxRow>,
            #(#bounds),*
        {
            fn from_row(
                __row: &'r __WormxRow,
            ) -> ::std::result::Result<Self, ::wormx::__private::Error> {
                ::std::result::Result::Ok(Self {
                    #(#init,)*
                    #(#relation_init,)*
                })
            }
        }
    }
}
