use proc_macro::TokenStream;

mod model;

#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    model::expand_model_attribute(attr, item)
}

#[proc_macro_derive(Model, attributes(seekwel))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    model::expand_model_derive(input)
}
