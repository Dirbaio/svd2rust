use log::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::common::*;
use crate::ir::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteFieldsets {
    pub from: String,
    #[serde(default)]
    pub useless: bool,
}

impl DeleteFieldsets {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let re = make_regex(&self.from)?;

        let mut ids: HashSet<Id<FieldSet>> = HashSet::new();
        for (id, fs) in ir.fieldsets.iter() {
            if path_matches(&fs.path, &re) && (!self.useless | is_useless(fs)) {
                info!("deleting fieldset {}", fs.path);
                ids.insert(id);
            }
        }

        remove_fieldset_ids(ir, &ids);
        for id in ids {
            ir.fieldsets.remove(id)
        }

        Ok(())
    }
}

fn is_useless(fs: &FieldSet) -> bool {
    match &fs.fields[..] {
        [] => true,
        [f] => fs.bit_size == f.bit_size && f.bit_offset == 0 && f.enumm.is_none(),
        _ => false,
    }
}

fn remove_fieldset_ids(ir: &mut IR, from: &HashSet<Id<FieldSet>>) {
    for (_, b) in ir.blocks.iter_mut() {
        for i in b.items.iter_mut() {
            if let BlockItemInner::Register(reg) = &mut i.inner {
                if let Some(id) = reg.fieldset {
                    if from.contains(&id) {
                        reg.fieldset = None
                    }
                }
            }
        }
    }
}
