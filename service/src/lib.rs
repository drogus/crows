use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use syn::parse::{Parse, ParseStream};

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use quote::quote;
use syn::spanned::Spanned;
use syn::token::{Comma, Mut};
use syn::{
    braced, parenthesized, parse_macro_input, parse_quote, Attribute, FnArg, Ident, LitStr, Pat,
    PatType, Result, ReturnType, Token, Type, Visibility,
};

/// Accumulates multiple errors into a result.
/// Only use this for recoverable errors, i.e. non-parse errors. Fatal errors should early exit to
/// avoid further complications.
macro_rules! extend_errors {
    ($errors: ident, $e: expr) => {
        match $errors {
            Ok(_) => $errors = Err($e),
            Err(ref mut errors) => errors.extend($e),
        }
    };
}

#[allow(dead_code)]
#[derive(Debug)]
struct ServiceMacroInput {
    attrs: Vec<Attribute>,
    vis: Visibility,
    ident: Ident,
    methods: Vec<Method>,
}

#[allow(dead_code)]
#[derive(Debug)]
struct Method {
    attrs: Vec<Attribute>,
    ident: Ident,
    args: Vec<PatType>,
    output: ReturnType,
    receiver: bool,
    receiver_mutability: Option<Mut>,
}

impl Parse for Method {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;

        input.parse::<Token![async]>()?;

        input.parse::<Token![fn]>()?;
        let ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        let mut args = Vec::new();
        let mut errors = Ok(());
        let mut receiver = false;
        let mut receiver_mutability = None;
        for arg in content.parse_terminated::<FnArg, Comma>(FnArg::parse)? {
            match arg {
                FnArg::Typed(captured) if matches!(&*captured.pat, Pat::Ident(_)) => {
                    args.push(captured);
                }
                FnArg::Typed(captured) => {
                    extend_errors!(
                        errors,
                        syn::Error::new(captured.pat.span(), "patterns aren't allowed in RPC args")
                    );
                }
                FnArg::Receiver(r) => {
                    receiver = true;
                    receiver_mutability = r.mutability.clone();
                }
            }
        }
        errors?;
        let output = input.parse()?;
        input.parse::<Token![;]>()?;

        Ok(Self {
            attrs,
            ident,
            args,
            output,
            receiver,
            receiver_mutability,
        })
    }
}

impl Parse for ServiceMacroInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![trait]>()?;
        let ident: Ident = input.parse()?;

        let mut methods = Vec::<Method>::new();

        let content;
        braced!(content in input);
        while !content.is_empty() {
            methods.push(content.parse()?);
        }

        Ok(Self {
            attrs,
            vis,
            ident,
            methods,
        })
    }
}

struct AttrsInput {
    other_side: Ident,
    variant: String,
}

impl Parse for AttrsInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut other_side: Option<Ident> = None;
        let mut variant: Option<String> = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            if ident == "other_side" {
                input.parse::<Token![=]>()?;
                other_side = Some(input.parse()?);
            } else if ident == "variant" {
                input.parse::<Token![=]>()?;
                let lit: LitStr = input.parse()?;
                variant = Some(lit.value());
            } else {
                return Err(syn::Error::new_spanned(ident, "Unexpected identifier"));
            }

            // Allow multiple attrs separated by comma.
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(AttrsInput {
            other_side: other_side.unwrap(),
            variant: variant.unwrap(),
        })
    }
}

#[proc_macro_attribute]
pub fn service(attr: TokenStream, original_input: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as AttrsInput);

    let derive = quote! {
        #[derive(Debug, utils::serde::Serialize, utils::serde::Deserialize, Clone)]
    };

    let cloned = original_input.clone();
    let input = parse_macro_input!(cloned as ServiceMacroInput);
    let unit_type: &Type = &parse_quote!(());

    let ident = input.ident;
    let request_ident = Ident::new(&format!("{}Request", ident), ident.span());
    let response_ident = Ident::new(&format!("{}Response", ident), ident.span());
    let message_ident = Ident::new(&format!("{}Message", ident), ident.span());
    let dummy_ident = Ident::new(&format!("Dummy{}Service", ident), ident.span());
    let client_ident = Ident::new(&format!("{}Client", ident), ident.span());
    let mut requests_variants = Vec::new();
    let mut requests_structs = Vec::new();
    let mut response_variants = Vec::new();
    let mut client_methods = Vec::new();
    let mut service_match_arms = Vec::new();

    let snake_ident = ident.to_string().to_case(Case::Snake);
    let variant = attrs.variant;
    #[allow(unused)]
    let create_named_variant_ident =
        Ident::new(&format!("create_{snake_ident}_{variant}"), ident.span());
    // let create_variant_ident = Ident::new(&format!("create_{variant}"), ident.span());
    let other_side = attrs.other_side;
    let other_side_client_ident = Ident::new(
        &format!("{}Client", other_side.to_string()),
        other_side.span(),
    );

    let server_or_client_fn = if &variant == "server" {
        let server_ident = Ident::new(&format!("{}Server", ident), ident.span());
        quote! {
            pub struct #server_ident {
                server: utils::Server
            }

            impl #server_ident {
                pub async fn accept<T>(&self, service: T) -> Option<#other_side_client_ident>
                where (T,): utils::Service<#dummy_ident> + 'static {
                    let (sender, receiver, close_receiver) = self.server.accept().await?;
                    let client = utils::Client::new(sender, receiver, (service,), Some(close_receiver));
                    Some(#other_side_client_ident {client})
                }
            }

            pub async fn #create_named_variant_ident<A>(addr: A)
                -> Result<#server_ident, std::io::Error>
                where
                    A: utils::tokio::net::ToSocketAddrs
            {
                let server = utils::create_server(addr).await?;
                Ok(#server_ident { server })
            }
        }
    } else {
        let other_side_snake = other_side.to_string().to_case(Case::Snake);
        let connect_to_ident = Ident::new(&format!("connect_to_{other_side_snake}"), ident.span());
        quote! {
            pub async fn #connect_to_ident<A, T>(addr: A, service: T)
                -> Result<#other_side_client_ident, std::io::Error>
                where
                    A: utils::tokio::net::ToSocketAddrs,
                    (T,): utils::Service<#dummy_ident> + 'static,
            {
                let (sender, mut receiver) = utils::create_client(addr).await?;
                let client = utils::Client::new(sender, receiver, (service,), None);
                Ok(#other_side_client_ident { client })
            }
        }
    };

    let mut trait_methods = Vec::new();

    for method in input.methods {
        let receiver = if method.receiver_mutability.is_some() {
            quote! { &mut self }
        } else {
            quote! { &self }
        };

        let pascal = method.ident.to_string().to_case(Case::Pascal);
        let method_ident = method.ident.clone();
        let method_request_ident =
            Ident::new(&format!("{}{}Request", ident, pascal), method.ident.span());
        let request_variant = quote! {
            #method_request_ident(#method_request_ident)
        };
        requests_variants.push(request_variant);

        let method_response_ident =
            Ident::new(&format!("{}{}Response", ident, pascal), method.ident.span());
        let return_ty = match method.output {
            ReturnType::Default => unit_type,
            ReturnType::Type(_, ref ty) => ty,
        };
        response_variants.push(quote! {
            #method_response_ident(#return_ty)
        });

        let mut args = Vec::new();
        let mut arg_names: Vec<Ident> = Vec::new();
        for arg in method.args.clone() {
            let ident = match *arg.pat {
                Pat::Ident(ident) => ident.ident,
                _ => unreachable!(),
            };
            arg_names.push(ident.clone());
            let ty = arg.ty;
            args.push(quote! {
                #ident: #ty
            });
        }
        requests_structs.push(quote! {
            #derive
            pub struct #method_request_ident {
                #(#args),*
            }
        });

        let args = method.args;
        let output_ty = match method.output {
            ReturnType::Type(_, ref t) => t,
            ReturnType::Default => unit_type,
        };

        client_methods.push(quote! {
            // TODO: this should not be anyhow, but rather io::error or sth along the lines
            pub async fn #method_ident(#receiver, #(#args),*) -> anyhow::Result<#output_ty> {
                let response = self.client
                    .request::<#request_ident, #response_ident>(#request_ident::#method_request_ident(
                        #method_request_ident { #(#arg_names),* },
                    )).await?;

                Ok(match response {
                    #response_ident::#method_response_ident(r) => r,
                    _ => unreachable!()
                })
            }
        });

        trait_methods.push(quote! {
            fn #method_ident(#receiver, #(#args),*) -> impl std::future::Future<Output = #output_ty> + Send;
        });

        service_match_arms.push(quote! {
            #request_ident::#method_request_ident(request) => #response_ident::#method_response_ident(self.0.#method_ident(#(request.#arg_names),*).await),
        });
    }

    let mut result = quote! {
        pub trait #ident {
            #(#trait_methods)*
        }
    };

    let impl_service = quote! {
        impl<T> utils::Service<#dummy_ident> for (T,)
        where T: #ident + Send + Sync {
            type Request = #request_ident;
            type Response = #response_ident;

            fn handle_request(
                &mut self,
                message: Self::Request,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Self::Response> + Send + '_>> {
                Box::pin(async {
                    match message {
                        #(#service_match_arms)*
                    }
                })
            }
        }
    };

    let get_close_receiver = quote! {
        pub fn get_close_receiver(&mut self) -> Option<tokio::sync::oneshot::Receiver<()>> {
                self.client.get_close_receiver()
            }
    };

    let generated = quote! {
        pub struct #dummy_ident;

        #derive
        pub enum #request_ident {
            #(#requests_variants),*
        }

        #derive
        pub enum #response_ident {
            #(#response_variants),*
        }

        #(#requests_structs)*

        #derive
        pub enum #message_ident {
            Request(#request_ident),
            Response(#response_ident),
        }

        pub struct #client_ident {
            // TODO: this should be prefixed
            client: utils::Client
        }

        impl #client_ident {
            pub async fn wait(&mut self) {
                self.client.wait().await;
            }

            #get_close_receiver

            #(#client_methods)*
        }

        #impl_service

        #server_or_client_fn
    };
    result.extend(generated);
    TokenStream::from(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        // let result = add(2, 2);
        // assert_eq!(result, 4);
    }
}
