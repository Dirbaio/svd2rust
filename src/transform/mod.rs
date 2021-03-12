use std::collections::{HashMap, HashSet};

use crate::ir::*;
use crate::util::{ToSanitizedPascalCase, ToSanitizedSnakeCase, ToSanitizedUpperCase};
use log::*;
use serde::{Deserialize, Serialize};

pub fn sanitize(ir: &mut IR) {
    for (_, d) in ir.devices.iter_mut() {
        sanitize_path(&mut d.path);
    }

    for (_, b) in ir.blocks.iter_mut() {
        sanitize_path(&mut b.path);
        for i in b.items.iter_mut() {
            i.name = i.name.to_sanitized_snake_case().to_string();
        }
    }

    for (_, fs) in ir.fieldsets.iter_mut() {
        sanitize_path(&mut fs.path);
        for f in fs.fields.iter_mut() {
            f.name = f.name.to_sanitized_snake_case().to_string();
        }
    }

    for (_, e) in ir.enums.iter_mut() {
        sanitize_path(&mut e.path);
        for v in e.variants.iter_mut() {
            v.name = v.name.to_sanitized_upper_case().to_string();
        }
    }
}

fn sanitize_path(p: &mut Path) {
    for s in &mut p.modules {
        *s = s.to_sanitized_snake_case().to_string();
    }
    p.name = p.name.to_sanitized_pascal_case().to_string();
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FindDuplicateEnums {}
impl FindDuplicateEnums {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let mut suggested = HashSet::new();

        for (id1, e1) in ir.enums.iter() {
            if suggested.contains(&id1) {
                continue;
            }

            let mut ids = Vec::new();
            for (id2, e2) in ir.enums.iter() {
                if id1 != id2 && mergeable_enums(e1, e2) {
                    ids.push(id2)
                }
            }

            if !ids.is_empty() {
                ids.push(id1);
                info!("Duplicated enums:");
                for id in ids {
                    suggested.insert(id);
                    info!("   {}", ir.enums.get(id).path);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeIdenticalEnums {}
impl MergeIdenticalEnums {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let mut suggested = HashSet::new();
        let mut merges = Vec::new();

        for (id1, e1) in ir.enums.iter() {
            if suggested.contains(&id1) {
                continue;
            }

            let mut ids = Vec::new();
            for (id2, e2) in ir.enums.iter() {
                if id1 != id2 && e1.path == e2.path && mergeable_enums(e1, e2) {
                    ids.push(id2)
                }
            }

            if !ids.is_empty() {
                ids.push(id1);
                for &id in &ids {
                    suggested.insert(id);
                }
                merges.push(ids);
            }
        }

        for merge in merges {
            let id = merge[0];
            let other_ids: HashSet<Id<Enum>> = merge[1..].iter().map(|x| *x).collect();
            replace_enum_ids(ir, &other_ids, id);
            for id in other_ids {
                ir.enums.remove(id)
            }
        }

        Ok(())
    }
}

fn mergeable_enums(a: &Enum, b: &Enum) -> bool {
    a.variants == b.variants
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FindDuplicateFieldsets {}
impl FindDuplicateFieldsets {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let mut suggested = HashSet::new();

        for (id1, fs1) in ir.fieldsets.iter() {
            if suggested.contains(&id1) {
                continue;
            }

            let mut ids = Vec::new();
            for (id2, fs2) in ir.fieldsets.iter() {
                if id1 != id2 && mergeable_fieldsets(fs1, fs2) {
                    ids.push(id2)
                }
            }

            if !ids.is_empty() {
                ids.push(id1);
                info!("Duplicated fieldsets:");
                for id in ids {
                    suggested.insert(id);
                    info!("   {}", ir.fieldsets.get(id).path);
                }
            }
        }

        Ok(())
    }
}

fn mergeable_fieldsets(a: &FieldSet, b: &FieldSet) -> bool {
    a.bit_size == b.bit_size && a.fields == b.fields
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeEnums {
    pub from: String,
    pub to: String,
}

impl MergeEnums {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let re = regex::Regex::new(&format!("^{}$", &self.from))?;
        let groups = path_groups(&ir.enums, &re, &self.to);

        for (to, group) in groups {
            info!("Merging enums, dest: {}", to);
            for id in &group {
                info!("   {}", ir.enums.get(*id).path);
            }
            self.merge_enums(ir, group, to);
        }

        Ok(())
    }

    fn merge_enums(&self, ir: &mut IR, ids: HashSet<Id<Enum>>, to: Path) {
        let mut e = ir.enums.get(*ids.iter().next().unwrap()).clone();

        for id in &ids {
            let e2 = ir.enums.get(*id);
            if !mergeable_enums(&e, e2) {
                panic!("mergeing nonmergeable enums");
            }
        }

        e.path = to;
        let final_id = ir.enums.put(e);
        replace_enum_ids(ir, &ids, final_id);
        for id in ids {
            ir.enums.remove(id)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteEnum {
    pub from: String,
}

impl DeleteEnum {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let re = regex::Regex::new(&self.from)?;

        let mut ids: HashSet<Id<Enum>> = HashSet::new();
        for (id, e) in ir.enums.iter() {
            if path_matches(&e.path, &re) {
                ids.insert(id);
            }
        }

        remove_enum_ids(ir, &ids);
        for id in ids {
            ir.enums.remove(id)
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeFieldsets {
    pub from: String,
    pub to: String,
}

impl MergeFieldsets {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let re = regex::Regex::new(&format!("^{}$", &self.from))?;
        let groups = path_groups(&ir.fieldsets, &re, &self.to);

        for (to, group) in groups {
            info!("Merging fieldsets, dest: {}", to);
            for id in &group {
                info!("   {}", ir.fieldsets.get(*id).path);
            }
            self.merge_fieldsets(ir, group, to);
        }

        Ok(())
    }

    fn merge_fieldsets(&self, ir: &mut IR, ids: HashSet<Id<FieldSet>>, to: Path) {
        let mut fs = ir.fieldsets.get(*ids.iter().next().unwrap()).clone();

        for id in &ids {
            let fs2 = ir.fieldsets.get(*id);
            if !mergeable_fieldsets(&fs, fs2) {
                panic!("mergeing nonmergeable fieldsets");
            }
        }

        fs.path = to;
        let final_id = ir.fieldsets.put(fs);
        replace_fieldset_ids(ir, &ids, final_id);
        for id in ids {
            ir.fieldsets.remove(id)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RenameFields {
    pub fieldset: String,
    pub from: String,
    pub to: String,
}

impl RenameFields {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let path_re = regex::Regex::new(&format!("^{}$", &self.fieldset))?;
        let re = regex::Regex::new(&format!("^{}$", &self.from))?;
        for id in match_paths(&ir.fieldsets, &path_re) {
            let fs = ir.fieldsets.get_mut(id);
            for f in &mut fs.fields {
                if let Some(name) = string_match_expand(&f.name, &re, &self.to) {
                    f.name = name;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MakeArray {
    pub block: String,
    pub from: String,
    pub to: String,
}

impl MakeArray {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let path_re = regex::Regex::new(&format!("^{}$", &self.block))?;
        let re = regex::Regex::new(&format!("^{}$", &self.from))?;
        for id in match_paths(&ir.blocks, &path_re) {
            let b = ir.blocks.get_mut(id);
            let groups = string_groups(b.items.iter().map(|f| f.name.clone()), &re, &self.to);
            for (to, group) in groups {
                info!("arrayizing to {}", to);

                // Grab all items into a vec
                let mut items = Vec::new();
                for i in b.items.iter().filter(|i| group.contains(&i.name)) {
                    items.push(i);
                }

                // Sort by offs
                items.sort_by_key(|i| i.byte_offset);
                for i in &items {
                    info!("    {}", i.name);
                }

                // todo check they're mergeable
                // todo check they're not arrays (arrays of arrays not supported)

                // Guess stride.
                let byte_offset = items[0].byte_offset;
                let len = items.len() as u32;
                let byte_stride = if len == 1 {
                    // If there's only 1 item, we can't know the stride, but it
                    // doesn't really matter!
                    0
                } else {
                    items[1].byte_offset - items[0].byte_offset
                };

                // Check the stride guess is OK

                if items
                    .iter()
                    .enumerate()
                    .any(|(n, i)| i.byte_offset != byte_offset + (n as u32) * byte_stride)
                {
                    panic!("arrayize: items are not evenly spaced")
                }

                info!("offs {} stride {}", byte_offset, byte_stride);

                let mut item = items[0].clone();

                // Remove all
                b.items.retain(|i| !group.contains(&i.name));

                // Create the new array item
                item.name = to;
                item.array = Some(Array { byte_stride, len });
                item.byte_offset = byte_offset;
                b.items.push(item);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MakeBlock {
    pub block: String,
    pub from: String,
    pub to_outer: String,
    pub to_block: String,
    pub to_inner: String,
}

impl MakeBlock {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        let path_re = regex::Regex::new(&format!("^{}$", &self.block))?;
        let re = regex::Regex::new(&format!("^{}$", &self.from))?;
        for id in match_paths(&ir.blocks, &path_re) {
            let b = ir.blocks.get_mut(id);
            let groups = string_groups(b.items.iter().map(|f| f.name.clone()), &re, &self.to_outer);
            for (to, group) in groups {
                let b = ir.blocks.get_mut(id);
                info!("blockifizing to {}", to);

                // Grab all items into a vec
                let mut items = Vec::new();
                for i in b.items.iter().filter(|i| group.contains(&i.name)) {
                    items.push(i);
                }

                // Sort by offs
                items.sort_by_key(|i| i.byte_offset);
                for i in &items {
                    info!("    {}", i.name);
                }

                // todo check they're mergeable
                // todo check they're not arrays (arrays of arrays not supported)

                let byte_offset = items[0].byte_offset;
                let len = items.len() as u32;
                let byte_stride = if len == 1 {
                    // If there's only 1 item, we can't know the stride, but it
                    // doesn't really matter!
                    0
                } else {
                    items[1].byte_offset - items[0].byte_offset
                };

                let b2 = Block {
                    path: Path::new_from_string(&self.to_block), // todo regex
                    description: None,
                    items: items
                        .iter()
                        .map(|&i| {
                            let mut i = i.clone();
                            i.name = string_match_expand(&i.name, &re, &self.to_inner).unwrap();
                            i.byte_offset -= byte_offset;
                            i
                        })
                        .collect(),
                };
                let b2_id = if let Some((id, b3)) = ir.blocks.find(|b| b.path == b2.path) {
                    // todo check blocks are mergeable
                    id
                } else {
                    ir.blocks.put(b2)
                };

                // Remove all items
                let b = ir.blocks.get_mut(id);
                b.items.retain(|i| !group.contains(&i.name));

                // Create the new block item
                b.items.push(BlockItem {
                    name: to,
                    description: None,
                    array: None,
                    byte_offset,
                    inner: BlockItemInner::Block(b2_id),
                });
            }
        }
        Ok(())
    }
}

fn match_paths<T: Pathed>(set: &Set<T>, re: &regex::Regex) -> HashSet<Id<T>> {
    let mut ids: HashSet<Id<T>> = HashSet::new();
    for (id, e) in set.iter() {
        if path_matches(e.path(), &re) {
            ids.insert(id);
        }
    }
    ids
}

fn path_groups<T: Pathed>(
    set: &Set<T>,
    re: &regex::Regex,
    to: &String,
) -> HashMap<Path, HashSet<Id<T>>> {
    let mut groups: HashMap<Path, HashSet<Id<T>>> = HashMap::new();
    for (id, e) in set.iter() {
        if let Some(to) = path_match_expand(e.path(), &re, to) {
            if let Some(v) = groups.get_mut(&to) {
                v.insert(id);
            } else {
                let mut s = HashSet::new();
                s.insert(id);
                groups.insert(to, s);
            }
        }
    }
    groups
}

fn string_groups(
    set: impl Iterator<Item = String>,
    re: &regex::Regex,
    to: &String,
) -> HashMap<String, HashSet<String>> {
    let mut groups: HashMap<String, HashSet<String>> = HashMap::new();
    for s in set {
        if let Some(to) = string_match_expand(&s, &re, to) {
            if let Some(v) = groups.get_mut(&to) {
                v.insert(s);
            } else {
                let mut v = HashSet::new();
                v.insert(s);
                groups.insert(to, v);
            }
        }
    }
    groups
}

fn path_matches(path: &Path, regex: &regex::Regex) -> bool {
    let path = path.to_string();
    regex.is_match(&path)
}

fn path_match_expand(path: &Path, regex: &regex::Regex, res: &str) -> Option<Path> {
    let path = path.to_string();
    let m = regex.captures(&path)?;
    let mut dst = String::new();
    m.expand(res, &mut dst);
    Some(Path::new_from_string(&dst))
}

fn string_match_expand(s: &str, regex: &regex::Regex, res: &str) -> Option<String> {
    let m = regex.captures(&s)?;
    let mut dst = String::new();
    m.expand(res, &mut dst);
    Some(dst)
}

pub fn replace_enum_ids(ir: &mut IR, from: &HashSet<Id<Enum>>, to: Id<Enum>) {
    for (_, fs) in ir.fieldsets.iter_mut() {
        for f in fs.fields.iter_mut() {
            if let Some(id) = f.enumm {
                if from.contains(&id) {
                    f.enumm = Some(to)
                }
            }
        }
    }
}

pub fn remove_enum_ids(ir: &mut IR, from: &HashSet<Id<Enum>>) {
    for (_, fs) in ir.fieldsets.iter_mut() {
        for f in fs.fields.iter_mut() {
            if let Some(id) = f.enumm {
                if from.contains(&id) {
                    f.enumm = None
                }
            }
        }
    }
}

pub fn replace_fieldset_ids(ir: &mut IR, from: &HashSet<Id<FieldSet>>, to: Id<FieldSet>) {
    for (_, b) in ir.blocks.iter_mut() {
        for i in b.items.iter_mut() {
            if let BlockItemInner::Register(r) = &mut i.inner {
                if let Some(id) = r.fieldset {
                    if from.contains(&id) {
                        r.fieldset = Some(to)
                    }
                }
            }
        }
    }
}

pub fn replace_block_ids(ir: &mut IR, from: &HashSet<Id<Block>>, to: Id<Block>) {
    for (_, d) in ir.devices.iter_mut() {
        for p in d.peripherals.iter_mut() {
            if from.contains(&p.block) {
                p.block = to
            }
        }
    }

    for (_, b) in ir.blocks.iter_mut() {
        for i in b.items.iter_mut() {
            if let BlockItemInner::Block(id) = &i.inner {
                if from.contains(&id) {
                    i.inner = BlockItemInner::Block(to)
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Transform {
    DeleteEnum(DeleteEnum),
    MergeEnums(MergeEnums),
    MergeFieldsets(MergeFieldsets),
    RenameFields(RenameFields),
    MakeArray(MakeArray),
    MakeBlock(MakeBlock),
    FindDuplicateEnums(FindDuplicateEnums),
    FindDuplicateFieldsets(FindDuplicateFieldsets),
}

impl Transform {
    pub fn run(&self, ir: &mut IR) -> anyhow::Result<()> {
        match self {
            Self::DeleteEnum(t) => t.run(ir),
            Self::MergeEnums(t) => t.run(ir),
            Self::MergeFieldsets(t) => t.run(ir),
            Self::RenameFields(t) => t.run(ir),
            Self::MakeArray(t) => t.run(ir),
            Self::MakeBlock(t) => t.run(ir),
            Self::FindDuplicateEnums(t) => t.run(ir),
            Self::FindDuplicateFieldsets(t) => t.run(ir),
        }
    }
}
