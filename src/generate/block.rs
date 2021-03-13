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

pub fn render(ir: &IR, b: &Block) -> Result<TokenStream> {
    let span = Span::call_site();
    let mut items = TokenStream::new();

    for i in &b.items {
        let name = Ident::new(&i.name, span);
        let offset = i.byte_offset as usize;

        let doc = util::doc(&i.description);

        match &i.inner {
            BlockItemInner::Register(r) => {
                let reg_ty = if let Some(f) = r.fieldset {
                    let f = ir.fieldsets.get(f);
                    util::relative_path(&f.path, &b.path)
                } else {
                    quote!(u32) // todo
                };

                let access = match r.access {
                    Access::Read => quote!(R),
                    Access::Write => quote!(W),
                    Access::ReadWrite => quote!(RW),
                };

                let ty = quote!(Reg<#reg_ty, #access>);
                if let Some(array) = &i.array {
                    let len = array.len as usize;
                    let stride = array.stride as usize;
                    items.extend(quote!(
                        #doc
                        pub fn #name(self, n: usize) -> #ty {
                            assert!(n < #len);
                            unsafe { Reg::from_ptr(self.0.add(#offset + n * #stride)) }
                        }
                    ));
                } else {
                    items.extend(quote!(
                        #doc
                        pub fn #name(self) -> #ty {
                            unsafe { Reg::from_ptr(self.0.add(#offset)) }
                        }
                    ));
                }
            }
            BlockItemInner::Block(b2) => {
                let b2 = ir.blocks.get(*b2);
                let ty = util::relative_path(&b2.path, &b.path);
                if let Some(array) = &i.array {
                    let len = array.len as usize;
                    let stride = array.stride as usize;
                    items.extend(quote!(
                        #doc
                        pub fn #name(self, n: usize) -> #ty {
                            assert!(n < #len);
                            unsafe { #ty(self.0.add(#offset + n * #stride)) }
                        }
                    ));
                } else {
                    items.extend(quote!(
                        #doc
                        pub fn #name(self) -> #ty {
                            unsafe { #ty(self.0.add(#offset)) }
                        }
                    ));
                }
            }
        }
    }

    let name = Ident::new(&b.path.name, span);
    let doc = util::doc(&b.description);
    let out = quote! {
        #doc
        #[derive(Copy, Clone)]
        pub struct #name (pub *mut u8);
        unsafe impl Send for #name {}
        unsafe impl Sync for #name {}
        impl #name {
            #items
        }
    };

    Ok(out)
}
