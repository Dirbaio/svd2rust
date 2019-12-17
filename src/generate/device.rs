use quote::ToTokens;
use proc_macro2::{TokenStream, Ident, Span};
use std::fs::File;
use std::io::Write;
use crate::svd::Device;

use crate::errors::*;
use crate::util::{self, ToSanitizedUpperCase, U32Ext};
use crate::Target;

use crate::generate::{interrupt, peripheral};

/// Whole device generation
pub fn render(
    d: &Device,
    target: Target,
    nightly: bool,
    generic_mod: bool,
    device_x: &mut String,
) -> Result<Vec<TokenStream>> {
    let mut out = vec![];

    let doc = format!(
        "Peripheral access API for {0} microcontrollers \
         (generated using svd2rust v{1})\n\n\
         You can find an overview of the API [here].\n\n\
         [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api",
        d.name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );

    if target == Target::Msp430 {
        out.push(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    if target != Target::None && target != Target::CortexM && target != Target::RISCV {
        out.push(quote! {
            #![cfg_attr(feature = "rt", feature(global_asm))]
            #![cfg_attr(feature = "rt", feature(use_extern_macros))]
            #![cfg_attr(feature = "rt", feature(used))]
        });
    }

    out.push(quote! {
        #![doc = #doc]
        // Deny a subset of warnings
        #![deny(const_err)]
        #![deny(dead_code)]
        #![deny(improper_ctypes)]
        #![deny(legacy_directory_ownership)]
        #![deny(missing_docs)]
        #![deny(no_mangle_generic_items)]
        #![deny(non_shorthand_field_patterns)]
        #![deny(overflowing_literals)]
        #![deny(path_statements)]
        #![deny(patterns_in_fns_without_body)]
        #![deny(plugin_as_library)]
        #![deny(private_in_public)]
        #![deny(safe_extern_statics)]
        #![deny(unconditional_recursion)]
        #![deny(unions_with_drop_fields)]
        #![deny(unused_allocation)]
        #![deny(unused_comparisons)]
        #![deny(unused_parens)]
        #![deny(while_true)]
        // Explicitly allow a few warnings that may be verbose
        #![allow(non_camel_case_types)]
        #![allow(non_snake_case)]
        #![no_std]
    });

    match target {
        Target::CortexM => {
            out.push(quote! {
                extern crate cortex_m;
                #[cfg(feature = "rt")]
                extern crate cortex_m_rt;
            });
        }
        Target::Msp430 => {
            out.push(quote! {
                extern crate msp430;
                #[cfg(feature = "rt")]
                extern crate msp430_rt;
                #[cfg(feature = "rt")]
                pub use msp430_rt::default_handler;
            });
        }
        Target::RISCV => {
            out.push(quote! {
                extern crate riscv;
                #[cfg(feature = "rt")]
                extern crate riscv_rt;
            });
        }
        Target::None => {}
    }

    out.push(quote! {
        extern crate bare_metal;
        extern crate vcell;

        use core::ops::Deref;
        use core::marker::PhantomData;
    });

    // Retaining the previous assumption
    let mut fpu_present = true;

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(u64::from(cpu.nvic_priority_bits));

        out.push(quote! {
            ///Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });

        fpu_present = cpu.fpu_present;
    }

    out.extend(interrupt::render(target, &d.peripherals, device_x)?);

    let core_peripherals: &[_] = if fpu_present {
        &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "FPU", "ITM", "MPU", "NVIC", "SCB", "SYST",
            "TPIU",
        ]
    } else {
        &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU",
        ]
    };

    let mut fields = vec![];
    let mut exprs = vec![];
    if target == Target::CortexM {
        out.push(quote! {
            pub use cortex_m::peripheral::Peripherals as CorePeripherals;
            #[cfg(feature = "rt")]
            pub use cortex_m_rt::interrupt;
            #[cfg(feature = "rt")]
            pub use self::Interrupt as interrupt;
        });

        if fpu_present {
            out.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, FPU, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        } else {
            out.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        }
    }

    let tokens = match target {
        Target::CortexM => generic_render(&[8, 16, 32])?,
        Target::Msp430 => generic_render(&[8, 16, 32])?,
        Target::RISCV => generic_render(&[32, 64])?,
        _ => generic_render(&[8, 16, 32, 64])?,
    };
    if generic_mod {
        writeln!(File::create("generic.rs").unwrap(), "{}", tokens).unwrap();
    } else {

        out.push(quote! {
            #[allow(unused_imports)]
            use generic::*;
            ///Common register and bit access and modify traits
            pub mod generic {
                #tokens
            }
        });
    }

    for p in &d.peripherals {
        if target == Target::CortexM && core_peripherals.contains(&&*p.name.to_uppercase()) {
            // Core peripherals are handled above
            continue;
        }

        out.extend(peripheral::render(p, &d.peripherals, &d.default_register_properties, nightly)?);

        if p.registers
            .as_ref()
            .map(|v| &v[..])
            .unwrap_or(&[])
            .is_empty()
            && p.derived_from.is_none()
        {
            // No register block will be generated so don't put this peripheral
            // in the `Peripherals` struct
            continue;
        }

        let p = p.name.to_sanitized_upper_case();
        let id = Ident::new(&p, Span::call_site());
        fields.push(quote! {
            #[doc = #p]
            pub #id: #id
        });
        exprs.push(quote!(#id: #id { _marker: PhantomData }));
    }

    let span = Span::call_site();
    let take = match target {
        Target::CortexM => Some(Ident::new("cortex_m", span)),
        Target::Msp430 => Some(Ident::new("msp430", span)),
        Target::RISCV => Some(Ident::new("riscv", span)),
        Target::None => None,
    }
    .map(|krate| {
        quote! {
            ///Returns all the peripherals *once*
            #[inline]
            pub fn take() -> Option<Self> {
                #krate::interrupt::free(|_| {
                    if unsafe { DEVICE_PERIPHERALS } {
                        None
                    } else {
                        Some(unsafe { Peripherals::steal() })
                    }
                })
            }
        }
    });

    out.push(quote! {
        // NOTE `no_mangle` is used here to prevent linking different minor versions of the device
        // crate as that would let you `take` the device peripherals more than once (one per minor
        // version)
        #[no_mangle]
        static mut DEVICE_PERIPHERALS: bool = false;

        ///All the peripherals
        #[allow(non_snake_case)]
        pub struct Peripherals {
            #(#fields,)*
        }

        impl Peripherals {
            #take

            ///Unchecked version of `Peripherals::take`
            #[inline]
            pub unsafe fn steal() -> Self {
                DEVICE_PERIPHERALS = true;

                Peripherals {
                    #(#exprs,)*
                }
            }
        }
    });

    Ok(out)
}

/// Generates generic bit munging code
pub fn generic_render(rsizes: &[u32]) -> Result<TokenStream> {
    let generic_file = std::str::from_utf8(include_bytes!("generic.rs")).unwrap();
    let tokens = syn::parse_file(generic_file).unwrap().into_token_stream();
    let mut code = vec![];
    code.push(tokens);

    let max_rsize = *rsizes.iter().max().unwrap();
    for i in 0..max_rsize {
        let oty = util::to_offset_ty(i as u64);
        let val = util::unsuffixed(i as u64);
        code.push(quote! {
            offset!(#oty, #val);
        });
    }

    for &fsize in &[8, 16, 32, 64] {
        if fsize > max_rsize {
            break;
        }
        let fty = fsize.to_ty()?;
        code.push(quote! {
            impl_bit_proxy!(#fty);
        });
    }

    for (i, rsize) in rsizes.iter().enumerate() {
        let rty = rsize.to_ty()?;
        for j in 0..=i {
            let fty = rsizes[j].to_ty()?;
            code.push(quote! {
                impl_proxy_safe!(#rty, #fty);
                impl_proxy_unsafe!(#rty, #fty);
            });
        }
    }
    Ok(quote! { #(#code)* })
}
