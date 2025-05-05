use darling::{
    FromDeriveInput, FromField, FromVariant,
    ast::{Data, Fields},
    util::Ignored,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{Attribute, DeriveInput, Error, Expr, ExprLit, Ident, Lit, Meta, parse_macro_input};

#[proc_macro_derive(ApiProblemDetails, attributes(oai_problemdetails))]
pub fn derive_response(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DeriveInput);

    match generate(args) {
        Ok(stream) => stream,
        Err(err) => err.write_errors().into(),
    }
}

fn generate(args: DeriveInput) -> Result<TokenStream, GeneratorError> {
    let args = ResponseArgs::from_derive_input(&args)?;

    let mut statuses = vec![];
    let mut responses_meta = vec![];
    let mut as_response = vec![];

    let ident = args.ident;

    let items = match args.data {
        Data::Enum(items) => items,
        Data::Struct(_) => {
            return Err(Error::new_spanned(
                ident,
                "error_response can only be applied to an enum.",
            )
            .into());
        }
    };

    for item in items {
        let item_ident = item.ident.clone();
        let status = get_status(item_ident.span(), &item.status)?;
        let fields = item
            .fields
            .iter()
            .map(|_| quote::quote! { _ })
            .collect::<Vec<_>>();

        let match_pattern = match item.fields.style {
            darling::ast::Style::Tuple => quote::quote! { #ident::#item_ident ( #(#fields),* ) },
            darling::ast::Style::Struct => quote::quote! { #ident::#item_ident { .. } },
            darling::ast::Style::Unit => quote::quote! { #ident::#item_ident },
        };

        statuses.push(quote::quote! {
			#match_pattern => poem::http::StatusCode::from_u16(#status).expect("Provided an invalid statuscode")
		});

        let description = get_description(&item.attrs)?
            .map(|tokens| quote::quote! { Some(#tokens) })
            .unwrap_or_else(|| quote::quote! { None::<&'static str> });

        let title = get_field(&item.title).unwrap_or_default();
        let title_property = schema_string("title", &title);

        let detail = get_field(&item.detail).unwrap_or_default();
        let detail_property = if detail.is_empty() {
            proc_macro2::TokenStream::default()
        } else {
            quote::quote! {
                ("detail", ::poem_openapi::registry::MetaSchemaRef::Inline(
                    ::std::boxed::Box::new(::poem_openapi::registry::MetaSchema {
                        ty: "string",
                        ..::poem_openapi::registry::MetaSchema::ANY
                    })
                )),
            }
        };

        let error_type = get_field(&item.ty).unwrap_or_else(|| {
            quote::quote! { "about:blank" }
        });
        let error_type_property = schema_string("type", &error_type);

        let status_property = schema_number("status", &status);

        responses_meta.push(quote::quote! {
            ::poem_openapi::registry::MetaResponse {
                description: #description.unwrap_or_default(),
                status: ::std::option::Option::Some(#status),
                content: ::std::vec![
                    ::poem_openapi::registry::MetaMediaType {
                        content_type: "application/problem+json",
                        schema: ::poem_openapi::registry::MetaSchemaRef::Inline(
                            ::std::boxed::Box::new(::poem_openapi::registry::MetaSchema {
                                properties: ::std::vec![
                                    #error_type_property
                                    #status_property
                                    #title_property
                                    #detail_property
                                ],
                                ..::poem_openapi::registry::MetaSchema::ANY
                            })
                        )
                    }
                ],
                status_range: None,
                headers: ::std::vec![],
            }
        });

        let with_title = if title.is_empty() {
            proc_macro2::TokenStream::default()
        } else {
            quote::quote! {
                .with_title(#title)
            }
        };

        let with_detail = if detail.is_empty() {
            proc_macro2::TokenStream::default()
        } else {
            println!("{item:?}");

            quote::quote! {
                .with_detail(format!(#detail, 18))
            }
        };

        as_response.push(quote::quote! {
            #match_pattern => {
                ::problemdetails::new(::poem::http::StatusCode::from_u16(#status).expect("An invalid status code was provided"))
                    .with_type(#error_type)
                    .with_value("status", #status)
                    #with_title
                    #with_detail
                    .into_response()
            }
        });
    }

    let stream = quote::quote! {
        impl ::poem_openapi::ApiResponse for #ident {
            fn meta() -> ::poem_openapi::registry::MetaResponses {
                ::poem_openapi::registry::MetaResponses {
                    responses: ::std::vec![#(#responses_meta),*]
                }
            }

            fn register(registry: &mut ::poem_openapi::registry::Registry) {
                <::poem_openapi::payload::Json<std::vec::Vec::<u8>> as ::poem_openapi::ResponseContent>::register(registry);
            }
        }

        impl ::poem::error::ResponseError for #ident {
            fn status(&self) -> ::poem::http::StatusCode {
                match &self {
                    #(#statuses),*
                }
            }

            fn as_response(&self) -> ::poem::Response {
                use poem::IntoResponse;

                match &self {
                    #(#as_response),*
                }
            }
        }
    };

    Ok(stream.into())
}

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(oai_problemdetails), forward_attrs(doc))]
struct ResponseArgs {
    ident: Ident,
    data: Data<ResponseItem, Ignored>,
}

#[derive(Debug, FromVariant)]
#[darling(attributes(oai_problemdetails), forward_attrs(doc))]
struct ResponseItem {
    ident: Ident,
    attrs: Vec<Attribute>,
    fields: Fields<ResponseField>,
    status: LitOrPath<u16>,
    #[darling(default)]
    title: Option<LitOrPath<String>>,
    #[darling(default)]
    detail: Option<LitOrPath<String>>,
    #[darling(default)]
    ty: Option<LitOrPath<String>>,
}

#[derive(Debug, FromField)]
#[darling(forward_attrs(doc))]
struct ResponseField {
    ident: Option<Ident>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GeneratorError {
    #[error("{0}")]
    Syn(#[from] syn::Error),
    #[error("{0}")]
    Darling(#[from] darling::Error),
}

impl GeneratorError {
    pub(crate) fn write_errors(self) -> proc_macro2::TokenStream {
        match self {
            GeneratorError::Syn(err) => err.to_compile_error(),
            GeneratorError::Darling(err) => err.write_errors(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum LitOrPath<T> {
    Lit(T),
    Path(syn::Path),
}

impl<T> darling::FromMeta for LitOrPath<T>
where
    T: darling::FromMeta,
{
    fn from_nested_meta(item: &darling::ast::NestedMeta) -> darling::Result<Self> {
        T::from_nested_meta(item)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_nested_meta(item).map(Self::Path))
    }

    fn from_meta(item: &syn::Meta) -> darling::Result<Self> {
        T::from_meta(item)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_meta(item).map(Self::Path))
    }

    fn from_none() -> Option<Self> {
        T::from_none()
            .map(Self::Lit)
            .or_else(|| syn::Path::from_none().map(Self::Path))
    }

    fn from_word() -> darling::Result<Self> {
        T::from_word()
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_word().map(Self::Path))
    }

    fn from_list(items: &[darling::ast::NestedMeta]) -> darling::Result<Self> {
        T::from_list(items)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_list(items).map(Self::Path))
    }

    fn from_value(value: &Lit) -> darling::Result<Self> {
        T::from_value(value)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_value(value).map(Self::Path))
    }

    fn from_expr(expr: &syn::Expr) -> darling::Result<Self> {
        T::from_expr(expr)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_expr(expr).map(Self::Path))
    }

    fn from_char(value: char) -> darling::Result<Self> {
        T::from_char(value)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_char(value).map(Self::Path))
    }

    fn from_string(value: &str) -> darling::Result<Self> {
        T::from_string(value)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_string(value).map(Self::Path))
    }

    fn from_bool(value: bool) -> darling::Result<Self> {
        T::from_bool(value)
            .map(Self::Lit)
            .or_else(|_| syn::Path::from_bool(value).map(Self::Path))
    }
}

fn get_status(
    span: Span,
    status: &LitOrPath<u16>,
) -> Result<proc_macro2::TokenStream, GeneratorError> {
    match status {
        LitOrPath::Lit(status) => {
            if !(100..1000).contains(status) {
                return Err(Error::new(
                    span,
                    "Invalid status code, it must be greater or equal to 100 and less than 1000.",
                )
                .into());
            }
            Ok(quote::quote!(#status))
        }
        LitOrPath::Path(ident) => Ok(quote::quote!(#ident)),
    }
}

pub(crate) fn get_description(attrs: &[Attribute]) -> Result<Option<String>, GeneratorError> {
    let mut full_docs = String::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(nv) = &attr.meta {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(doc), ..
                }) = &nv.value
                {
                    let doc = doc.value();
                    let doc_str = doc.trim();
                    if !full_docs.is_empty() {
                        full_docs += "\n";
                    }
                    full_docs += doc_str;
                }
            }
        }
    }
    Ok(if full_docs.is_empty() {
        None
    } else {
        Some(full_docs)
    })
}

fn get_field(field: &Option<LitOrPath<String>>) -> Option<proc_macro2::TokenStream> {
    match field {
        Some(LitOrPath::Lit(lit)) => Some(quote::quote!(#lit)),
        Some(LitOrPath::Path(path)) => Some(quote::quote!(#path)),
        None => None,
    }
}

fn schema_string(name: &'static str, value: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    if value.is_empty() {
        proc_macro2::TokenStream::default()
    } else {
        quote::quote! {
            (#name, ::poem_openapi::registry::MetaSchemaRef::Inline(
                ::std::boxed::Box::new(::poem_openapi::registry::MetaSchema {
                    ty: "string",
                    enum_items: ::std::vec![
                        ::serde_json::Value::String(#value.into())
                    ],
                    ..::poem_openapi::registry::MetaSchema::ANY
                })
            )),
        }
    }
}

fn schema_number(name: &'static str, value: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    if value.is_empty() {
        proc_macro2::TokenStream::default()
    } else {
        quote::quote! {
            (#name, ::poem_openapi::registry::MetaSchemaRef::Inline(
                ::std::boxed::Box::new(::poem_openapi::registry::MetaSchema {
                    ty: "number",
                    enum_items: ::std::vec![
                        ::serde_json::Value::Number(#value.into())
                    ],
                    ..::poem_openapi::registry::MetaSchema::ANY
                })
            )),
        }
    }
}
