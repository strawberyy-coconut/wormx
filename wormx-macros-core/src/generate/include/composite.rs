use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::super::model::{Relation, RelationKind};
use super::super::naming::relation_key_ident;

pub(super) fn include_body_composite(relation: &Relation, in_transaction: bool) -> TokenStream {
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
