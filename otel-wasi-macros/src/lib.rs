use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, ItemFn, LitStr, ReturnType, Token, Type, parenthesized, parse::Parse,
    parse::ParseStream, parse_macro_input,
};

struct InstrumentArgs {
    service: Option<LitStr>,
    name: Option<LitStr>,
    error_slug: Option<LitStr>,
    attributes: Vec<(LitStr, Expr)>,
}

impl Parse for InstrumentArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self {
            service: None,
            name: None,
            error_slug: None,
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
                "error_slug" => {
                    input.parse::<Token![=]>()?;
                    args.error_slug = Some(input.parse()?);
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
    input: ItemFn,
) -> syn::Result<proc_macro2::TokenStream> {
    if input.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            input.sig.asyncness,
            "#[wasi_instrument] does not support async functions yet",
        ));
    }

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
    let default_error_slug = format!("{}-failed", span_name.value());
    let error_slug = args
        .error_slug
        .unwrap_or_else(|| LitStr::new(&default_error_slug, span_name.span()));

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

    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let outer_attrs = &input.attrs;

    let finish = match &sig.output {
        ReturnType::Default => quote! {
            let __otel_wasi_result = (|| {
                let __otel_wasi_guard = __otel_wasi_span.enter();
                #record_attrs
                #block
            })();
            __otel_wasi_span.finish(&__otel_wasi_result);
            __otel_wasi_result
        },
        ReturnType::Type(_, ty) if is_result_type(ty) => quote! {
            let __otel_wasi_result = (|| -> #ty {
                let __otel_wasi_guard = __otel_wasi_span.enter();
                #record_attrs
                #block
            })();
            __otel_wasi_span.finish(&__otel_wasi_result);
            __otel_wasi_result
        },
        ReturnType::Type(_, ty) => quote! {
            let __otel_wasi_result = (|| -> #ty {
                let __otel_wasi_guard = __otel_wasi_span.enter();
                #record_attrs
                #block
            })();
            __otel_wasi_span.finish_ok();
            __otel_wasi_result
        },
    };

    Ok(quote! {
        #(#outer_attrs)*
        #vis #sig {
            let __otel_wasi_span = ::otel_wasi::WasiSpan::start(
                ::otel_wasi::SpanConfig::builder()
                    .service_name(#service)
                    .span_name(#span_name)
                    .error_slug(#error_slug)
                    .build(),
            );

            #finish
        }
    })
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
