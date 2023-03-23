extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree, Literal};

#[proc_macro]
pub fn dash_conversion(input: TokenStream) -> TokenStream {
    match input.into_iter().next() {
        Some(TokenTree::Ident(name)) => {
            Into::<TokenTree>::into(Literal::string(&name.to_string().replace("_", "-"))).into()
        }
        got => panic!("expected ident; got: {got:?}"),
    }
}
