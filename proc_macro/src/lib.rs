use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Ident, Token, parse_macro_input};

#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream
{
    let input = parse_macro_input!(input as DeriveInput);
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let name = &input.ident;

    let flags: Vec<Ident> = if args.is_empty()
    {
        vec![]
    }
    else
    {
        let parser = Punctuated::<Ident, Token![,]>::parse_terminated;
        match syn::parse::Parser::parse(parser, args)
        {
            Ok(parsed) => parsed.into_iter().collect(),
            Err(e) => return e.to_compile_error().into(),
        }
    };

    const KNOWN: &[&str] = &["shared"];
    for flag in &flags
    {
        if !KNOWN.contains(&flag.to_string().as_str())
        {
            return syn::Error::new(flag.span(), format!("unknown component flag `{flag}`; expected one of: {KNOWN:?}"))
                .to_compile_error()
                .into();
        }
    }

    if let Some(flag) = flags.iter().find(|f| *f == "shared")
    {
        return syn::Error::new(
            flag.span(),
            "shared components are not declared with #[component]; implement xynok_ecs::shared::TSharedComponent and add them with World::add_shared_component",
        )
        .to_compile_error()
        .into();
    }
    let location = quote! { xynok_ecs::apis::identifies::StorageLocation::Chunk };

    let expanded = quote! {
        #input

        impl #impl_generics xynok_ecs::apis::traits::TComponent for #name #ty_generics #where_clause
        {
            type QueryType = Self;
            type StorageType = Self;
            const STORAGE_LOCATION: xynok_ecs::apis::identifies::StorageLocation = #location;
        }
    };

    TokenStream::from(expanded)
}
