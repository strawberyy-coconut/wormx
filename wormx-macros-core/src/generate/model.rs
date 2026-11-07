use std::collections::HashSet;

use proc_macro2::TokenStream;
use wormx_core::{RelationMode, TableRef};

pub(super) struct Field {
    pub(super) ident: syn::Ident,
    pub(super) db_name: String,
    pub(super) inner_ty: syn::Type,
    pub(super) ty: syn::Type,
    pub(super) nullable: bool,
    pub(super) primary_key: bool,
    pub(super) doc: String,
    pub(super) attrs: Vec<TokenStream>,
}

#[derive(Default)]
pub(super) struct CodegenPolicy {
    pub(super) relations: RelationMode,
    pub(super) include_relations: HashSet<String>,
    pub(super) skip_relations: HashSet<String>,
    pub(super) skip_columns: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RelationKind {
    BelongsTo,
    HasMany,
}

pub(super) struct Relation {
    pub(super) kind: RelationKind,
    pub(super) field_ident: syn::Ident,
    pub(super) field_name: String,
    pub(super) storage_ty: syn::Type,
    pub(super) target_ty: syn::Type,
    pub(super) include_ident: syn::Ident,
    pub(super) handle_ident: syn::Ident,
    pub(super) fk_field: Option<syn::Ident>,
    pub(super) key_ty: syn::Type,
    pub(super) key_nullable: bool,
    pub(super) source_key_field: Option<syn::Ident>,
    pub(super) source_key_ty: Option<syn::Type>,
    pub(super) target_fk_column: Option<String>,
    pub(super) target_fk_accessor: Option<syn::Ident>,
    pub(super) target_fk_nullable: bool,
    pub(super) target_pk_accessor: Option<syn::Ident>,
    pub(super) target_table: TableRef,
    pub(super) source_columns: Vec<String>,
    pub(super) target_columns: Vec<String>,
    /// Fields on this row that make up the relation key, in FK order.
    pub(super) source_key_fields: Vec<syn::Ident>,
    /// Types of `source_key_fields`.
    pub(super) source_key_tys: Vec<syn::Type>,
    /// Whether each source key field is nullable.
    pub(super) source_key_nullable: Vec<bool>,
    /// Hidden accessors on the target row for the key columns.
    pub(super) target_key_accessors: Vec<syn::Ident>,
    /// Whether each target key field is nullable.
    pub(super) target_key_nullable: Vec<bool>,
}

impl Relation {
    pub(super) fn is_composite(&self) -> bool {
        self.source_key_fields.len() > 1
    }

    pub(super) fn is_optional(&self) -> bool {
        self.kind == RelationKind::BelongsTo && self.source_key_nullable.iter().any(|value| *value)
    }
}
