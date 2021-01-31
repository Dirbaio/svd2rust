use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use log::warn;
use proc_macro2::TokenStream;
use proc_macro2::{Ident, Punct, Spacing, Span};
use quote::{quote, ToTokens};
use svd_parser::derive_from::DeriveFrom;

use crate::util;
use anyhow::{anyhow, bail, Context, Result};

use crate::ir::*;

pub fn render(d: &Device, e: &Enum) -> Result<TokenStream> {
    let span = Span::call_site();
    let mut items = TokenStream::new();

    let ty = match e.bit_size {
        1..=8 => quote!(u8),
        9..=16 => quote!(u16),
        17..=32 => quote!(u32),
        33..=64 => quote!(u64),
        _ => panic!("Invalid bit_size {}", e.bit_size),
    };

    for f in &e.variants {
        let name = Ident::new(&f.name, span);
        let value = util::hex(f.value);
        items.extend(quote!(
            pub const #name: Self = Self(#value);
        ));
    }

    let name = Ident::new(&e.path.name, span);

    let out = quote! {
        #[repr(transparent)]
        #[derive(Copy, Clone)]
        pub struct #name (#ty);

        impl #name {
            pub const fn to_bits(&self) -> #ty {
                self.0
            }
            pub const fn from_bits(val: #ty) -> #name {
                #name(val)
            }

            #items
        }
    };

    Ok(out)
}
