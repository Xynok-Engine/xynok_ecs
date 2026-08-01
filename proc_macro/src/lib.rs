use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, DeriveInput, Ident, Token};

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

    let is_archetype = flags.iter().any(|f| f == "archetype");

    let location = if is_archetype
    {
        quote! { xynok_ecs::apis::identifies::StorageLocation::Archetype }
    }
    else
    {
        quote! { xynok_ecs::apis::identifies::StorageLocation::Chunk }
    };

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
