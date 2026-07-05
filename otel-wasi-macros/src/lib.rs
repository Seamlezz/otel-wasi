use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, ItemFn, LitStr, ReturnType, Token, Type, parenthesized, parse::Parse,
    parse::ParseStream, parse_macro_input, parse_quote,
};

struct InstrumentArgs {
    service: Option<LitStr>,
    name: Option<LitStr>,
    export: bool,
    attributes: Vec<(LitStr, Expr)>,
}

impl Parse for InstrumentArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self {
            service: None,
            name: None,
            export: false,
            attributes: Vec::new(),
        };

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "service" => {
                    input.parse::<Token![=]>()?;
                    args.service = Some(input.parse()?);
                }
                "name" => {
                    input.parse::<Token![=]>()?;
                    args.name = Some(input.parse()?);
                }
                "export" => {
                    args.export = true;
                }
                "attributes" => {
                    let content;
                    parenthesized!(content in input);
                    while !content.is_empty() {
                        let key: LitStr = content.parse()?;
                        content.parse::<Token![=]>()?;
                        let value: Expr = content.parse()?;
                        args.attributes.push((key, value));

                        if content.is_empty() {
                            break;
                        }
                        content.parse::<Token![,]>()?;
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unsupported wasi_instrument option `{other}`"),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(args)
    }
}

#[proc_macro_attribute]
pub fn wasi_instrument(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as InstrumentArgs);
    let input = parse_macro_input!(item as ItemFn);

    expand_wasi_instrument(args, input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_wasi_instrument(
    args: InstrumentArgs,
    mut input: ItemFn,
) -> syn::Result<proc_macro2::TokenStream> {
    let service = args.service.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.sig.ident,
            "#[wasi_instrument] requires `service = \"...\"`",
        )
    })?;

    let fn_name = input.sig.ident.to_string().replace('_', "-");
    let span_name = args
        .name
        .unwrap_or_else(|| LitStr::new(&fn_name, input.sig.ident.span()));

    let attrs = args.attributes.iter().map(|(key, value)| {
        quote! { #key = #value }
    });
    let record_attrs = if args.attributes.is_empty() {
        quote! {}
    } else {
        quote! {
            ::otel_wasi::attribute!(#(#attrs),*);
        }
    };

    // Handle export mode: the body returns Result<T, otel_wasi::Error<E>>
    // (often with E defaulting to String). The exported WIT signature must
    // return Result<T, E>, so we rewrite the function signature and extract
    // the inner error at the boundary.
    let original_ret_ty = if args.export {
        let ret_ty: Type = match &input.sig.output {
            ReturnType::Type(_, ty) => (**ty).clone(),
            ReturnType::Default => {
                return Err(syn::Error::new_spanned(
                    &input.sig.ident,
                    "#[wasi_instrument(export)] requires a `Result<T, E>` return type",
                ));
            }
        };

        let (ok_ty, err_ty) = extract_result_types(&ret_ty).ok_or_else(|| {
            syn::Error::new_spanned(
                &ret_ty,
                "#[wasi_instrument(export)] requires a `Result<T, E>` return type",
            )
        })?;

        let export_err_ty = extract_otel_error_inner(err_ty).ok_or_else(|| {
            syn::Error::new_spanned(
                err_ty,
                "#[wasi_instrument(export)] error type must be `otel_wasi::Error<E>` (e.g. `otel_wasi::Error` or `otel_wasi::Error<ErrorCode>`)",
            )
        })?;

        // Rewrite the function signature to return Result<T, E>
        let new_ret_ty: Type = parse_quote! { Result<#ok_ty, #export_err_ty> };
        input.sig.output =
            ReturnType::Type(Token![->](input.sig.ident.span()), Box::new(new_ret_ty));

        Some(ret_ty)
    } else {
        None
    };

    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let outer_attrs = &input.attrs;

    let finish = if sig.asyncness.is_some() {
        expand_async_finish(&input.sig.output, block, &record_attrs, original_ret_ty)
    } else {
        expand_sync_finish(&input.sig.output, block, &record_attrs, original_ret_ty)
    };

    Ok(quote! {
        #(#outer_attrs)*
        #vis #sig {
            let __otel_wasi_span = {
                let __otel_wasi_config = ::otel_wasi::SpanConfig::builder()
                    .service_name(#service)
                    .span_name(#span_name)
                    .build();
                let __otel_wasi_tracing_span = ::otel_wasi::span!(
                    ::tracing::Level::INFO,
                    #span_name,
                );
                ::otel_wasi::WasiSpan::from_span(__otel_wasi_tracing_span, __otel_wasi_config)
            };

            #finish
        }
    })
}

/// Extract `(T, E)` from `Result<T, E>`.
fn extract_result_types(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last()?;
            if segment.ident != "Result" {
                return None;
            }
            match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => {
                    let mut types = args.args.iter().filter_map(|arg| match arg {
                        syn::GenericArgument::Type(ty) => Some(ty),
                        _ => None,
                    });
                    let ok = types.next()?;
                    let err = types.next()?;
                    Some((ok, err))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extract the inner type `E` from `otel_wasi::Error<E>` or `Error<E>`.
fn extract_otel_error_inner(ty: &Type) -> Option<Type> {
    let type_path = match ty {
        Type::Path(type_path) => type_path,
        _ => return None,
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Error" {
        return None;
    }
    match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) => match args.args.first()? {
            syn::GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        },
        // Bare `Error` with default generic parameter maps to `String`.
        syn::PathArguments::None => Some(parse_quote!(String)),
        _ => None,
    }
}

fn expand_sync_finish(
    output: &ReturnType,
    block: &syn::Block,
    record_attrs: &proc_macro2::TokenStream,
    export_original_ty: Option<Type>,
) -> proc_macro2::TokenStream {
    // In export mode, the inner closure uses the original `otel_wasi::Error<E>`
    // type and we extract the inner `E` at the boundary.
    let inner_ty: &Type = match &export_original_ty {
        Some(ty) => ty,
        None => match output {
            ReturnType::Type(_, ty) => ty,
            ReturnType::Default => {
                return quote! {
                    let __otel_wasi_result = (|| {
                        let __otel_wasi_main_guard = ::otel_wasi::enter_main_span(__otel_wasi_span.span().clone());
                        let __otel_wasi_guard = __otel_wasi_span.enter();
                        #record_attrs
                        #block
                    })();
                    __otel_wasi_span.finish(&__otel_wasi_result);
                    __otel_wasi_result
                };
            }
        },
    };

    let is_result = is_result_type(inner_ty);

    if !is_result {
        return quote! {
            let __otel_wasi_result = (|| -> #inner_ty {
                let __otel_wasi_main_guard = ::otel_wasi::enter_main_span(__otel_wasi_span.span().clone());
                let __otel_wasi_guard = __otel_wasi_span.enter();
                #record_attrs
                #block
            })();
            __otel_wasi_span.finish_ok();
            __otel_wasi_result
        };
    }

    let body = quote! {
        let __otel_wasi_result = (|| -> #inner_ty {
            let __otel_wasi_main_guard = ::otel_wasi::enter_main_span(__otel_wasi_span.span().clone());
            let __otel_wasi_guard = __otel_wasi_span.enter();
            #record_attrs
            #block
        })();
        __otel_wasi_span.finish(&__otel_wasi_result);
    };

    if export_original_ty.is_some() {
        quote! {
            #body
            match __otel_wasi_result {
                Ok(v) => Ok(v),
                Err(e) => Err(e.into_inner()),
            }
        }
    } else {
        quote! {
            #body
            __otel_wasi_result
        }
    }
}

fn expand_async_finish(
    output: &ReturnType,
    block: &syn::Block,
    record_attrs: &proc_macro2::TokenStream,
    export_original_ty: Option<Type>,
) -> proc_macro2::TokenStream {
    let inner_ty: &Type = match &export_original_ty {
        Some(ty) => ty,
        None => match output {
            ReturnType::Type(_, ty) => ty,
            ReturnType::Default => {
                return quote! {
                    let __otel_wasi_poll_span = __otel_wasi_span.span().clone();
                    let __otel_wasi_future = ::tracing::Instrument::instrument(async {
                        #record_attrs
                        #block
                    }, __otel_wasi_poll_span.clone());
                    let __otel_wasi_result = ::otel_wasi::with_main_span(__otel_wasi_poll_span, __otel_wasi_future).await;
                    __otel_wasi_span.finish(&__otel_wasi_result);
                    __otel_wasi_result
                };
            }
        },
    };

    let is_result = is_result_type(inner_ty);

    if !is_result {
        return quote! {
            let __otel_wasi_poll_span = __otel_wasi_span.span().clone();
            let __otel_wasi_future = ::tracing::Instrument::instrument(async {
                #record_attrs
                #block
            }, __otel_wasi_poll_span.clone());
            let __otel_wasi_result = ::otel_wasi::with_main_span(__otel_wasi_poll_span, __otel_wasi_future).await;
            __otel_wasi_span.finish_ok();
            __otel_wasi_result
        };
    }

    let body = quote! {
        let __otel_wasi_poll_span = __otel_wasi_span.span().clone();
        let __otel_wasi_future = ::tracing::Instrument::instrument(async {
            #record_attrs
            #block
        }, __otel_wasi_poll_span.clone());
        let __otel_wasi_result = ::otel_wasi::with_main_span(__otel_wasi_poll_span, __otel_wasi_future).await;
        __otel_wasi_span.finish(&__otel_wasi_result);
    };

    if export_original_ty.is_some() {
        quote! {
            #body
            match __otel_wasi_result {
                Ok(v) => Ok(v),
                Err(e) => Err(e.into_inner()),
            }
        }
    } else {
        quote! {
            #body
            __otel_wasi_result
        }
    }
}

fn is_result_type(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Result"),
        _ => false,
    }
}
