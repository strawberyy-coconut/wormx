use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::model::{Relation, RelationKind};
use super::naming::relation_key_ident;

pub(super) fn quote_composite_handle(relation: &Relation) -> TokenStream {
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
