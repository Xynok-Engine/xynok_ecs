use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, DeriveInput, Ident, Token};

const KNOWN: &[&str] = &["EnableAble", "ChangeAble"];

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

    for flag in &flags
    {
        if !KNOWN.contains(&flag.to_string().as_str())
        {
            return syn::Error::new(flag.span(), format!("unknown component flag `{flag}`; expected one of: {KNOWN:?}"))
                .to_compile_error()
                .into();
        }
    }

    let location = quote! { xynok_ecs::apis::identifies::StorageLocation::Chunk };
    let (enableable, changeable, state_detection) = detect_component_state(&flags);

    let enable = if enableable
    {
        quote! {
            impl #impl_generics xynok_ecs::apis::traits::TEnableAble for #name #ty_generics #where_clause
            {
            }
        }
    }
    else
    {
        quote! {}
    };

    let change = if changeable
    {
        quote! {
            impl #impl_generics xynok_ecs::apis::traits::TChangeAble for #name #ty_generics #where_clause
            {
            }
        }
    }
    else
    {
        quote! {}
    };
    let expanded = quote! {
        #input

        impl #impl_generics xynok_ecs::apis::traits::TComponent for #name #ty_generics #where_clause
        {
            type QueryType = Self;
            type StorageType = Self;
            const STORAGE_LOCATION: xynok_ecs::apis::identifies::StorageLocation = #location;
            const STATE_DETECTION: xynok_ecs::apis::identifies::StateDetection = #state_detection;
        }
        #enable
        #change
    };

    TokenStream::from(expanded)
}

fn detect_component_state(flags: &[Ident]) -> (bool, bool, proc_macro2::TokenStream)
{
    let is_enableable = flags.iter().any(|f| f == "EnableAble");
    let is_changeable = flags.iter().any(|f| f == "ChangeAble");

    let result = if is_enableable && is_changeable
    {
        quote! {xynok_ecs::apis::identifies::StateDetection::EnableAbleAndChangeAble}
    }
    else if is_enableable
    {
        quote! {xynok_ecs::apis::identifies::StateDetection::EnableAble}
    }
    else if is_changeable
    {
        quote! {xynok_ecs::apis::identifies::StateDetection::ChangeAble}
    }
    else
    {
        quote! {xynok_ecs::apis::identifies::StateDetection::None}
    };
    (is_enableable, is_changeable, result)
}
