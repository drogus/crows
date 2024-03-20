extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn config(_: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let generated_fn_name = syn::Ident::new(&format!("__{}", fn_name), fn_name.span());
    let _ = &input_fn.vis;

    let expanded = quote! {
        #input_fn

        #[export_name = "__config"]
        pub fn #generated_fn_name() -> u32 {
            crows_bindings::__set_config(#fn_name())
        }
    };

    TokenStream::from(expanded)
}
