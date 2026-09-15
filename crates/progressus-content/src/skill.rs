//! Stable identities for practical work skills; experience belongs to people.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkillDefinition {
    pub name: &'static str,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static SKILLS: &[SkillDefinition] = &[
    SkillDefinition { name: "gathering" },
    SkillDefinition { name: "mining" },
    SkillDefinition { name: "crafting" },
];

content_handle!(SkillId, SkillDefinition, SKILLS, skill);

pub const GATHERING: SkillId = skill("gathering");
pub const MINING: SkillId = skill("mining");
pub const CRAFTING: SkillId = skill("crafting");
