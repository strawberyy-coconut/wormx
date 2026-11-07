use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::super::model::{Relation, RelationKind};
use super::composite::include_body_composite;

pub(super) fn include_body(relation: &Relation, in_transaction: bool) -> TokenStream {
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
