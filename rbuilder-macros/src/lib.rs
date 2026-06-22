//! Procedural macros for rBuilder language plugins (Phase 7.2)

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, LitStr};

/// Generate boilerplate methods for a language plugin struct.
///
/// Implement `LanguagePlugin` manually, delegating `language_id`, `file_extensions`,
/// and `grammar` to the generated inherent methods.
#[proc_macro_derive(LanguagePlugin, attributes(language_plugin))]
pub fn derive_language_plugin(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let lang_attr = match input
        .attrs
        .iter()
        .find(|a| a.path().is_ident("language_plugin"))
    {
        Some(a) => a,
        None => {
            return syn::Error::new_spanned(
                &input.ident,
                "Missing #[language_plugin(...)] attribute",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut language_id = String::from("unknown");
    let mut extensions: Vec<String> = Vec::new();
    let mut grammar_crate = None;

    if let Ok(nested) = lang_attr
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    {
        for meta in nested {
            if let syn::Meta::NameValue(nv) = meta {
                if nv.path.is_ident("id") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = nv.value
                    {
                        language_id = s.value();
                    }
                } else if nv.path.is_ident("grammar") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = nv.value
                    {
                        grammar_crate = Some(s.value());
                    }
                }
            } else if let syn::Meta::List(list) = meta {
                if list.path.is_ident("extensions") {
                    let _ = list.parse_nested_meta(|meta| {
                        if let syn::Lit::Str(s) = meta.value()?.parse()? {
                            extensions.push(s.value());
                        }
                        Ok(())
                    });
                }
            }
        }
    }

    let ext_literals: Vec<LitStr> = extensions
        .iter()
        .map(|e| LitStr::new(e, proc_macro2::Span::call_site()))
        .collect();

    let has_parser_field =
        matches!(input.data, Data::Struct(ref ds) if matches!(ds.fields, Fields::Named(_)));

    let new_impl = if let Some(ref grammar) = grammar_crate {
        if has_parser_field {
            quote! {
                pub fn new() -> Result<Self, crate::error::Error> {
                    let mut parser = tree_sitter::Parser::new();
                    parser
                        .set_language(&#grammar::LANGUAGE.into())
                        .map_err(|e| crate::error::Error::PluginError(
                            format!("Failed to set grammar for {}: {}", #language_id, e)
                        ))?;
                    Ok(Self { _parser: parser })
                }
            }
        } else {
            quote! {
                pub fn new() -> Result<Self, crate::error::Error> {
                    Ok(Self)
                }
            }
        }
    } else {
        quote! {
            pub fn new() -> Result<Self, crate::error::Error> {
                Ok(Self)
            }
        }
    };

    let grammar_method = if let Some(grammar) = grammar_crate {
        quote! {
            pub fn grammar(&self) -> Option<tree_sitter::Language> {
                Some(#grammar::LANGUAGE.into())
            }
        }
    } else {
        quote! {
            pub fn grammar(&self) -> Option<tree_sitter::Language> {
                None
            }
        }
    };

    quote! {
        impl #name {
            #new_impl

            pub fn language_id(&self) -> &str {
                #language_id
            }

            pub fn file_extensions(&self) -> Vec<&str> {
                vec![#(#ext_literals),*]
            }

            #grammar_method
        }
    }
    .into()
}
