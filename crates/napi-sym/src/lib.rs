// Simplified napi_sym proc macro for howth.
// Wraps N-API function implementations with the napi_wrap! macro
// which adds #[no_mangle], extern "C", TryCatch handling, etc.

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn napi_sym(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = syn::parse::<syn::ItemFn>(item).expect("expected a function");
    TokenStream::from(quote! {
        crate::napi_wrap! {
            #func
        }
    })
}
