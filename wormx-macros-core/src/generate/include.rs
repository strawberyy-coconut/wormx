mod composite;
mod single;

use proc_macro2::TokenStream;
use quote::quote;

use super::model::Relation;
use single::include_body;

pub(super) fn quote_include_impls(
    struct_ident: &syn::Ident,
    relations: &[Relation],
) -> TokenStream {
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
