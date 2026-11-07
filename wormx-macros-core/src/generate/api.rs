use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::composite::quote_composite_handle;
use super::model::{Field, Relation, RelationKind};
use super::naming::column_accessor_ident;

pub(super) fn quote_api_impl(
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
