//! Settlement knowledge unlocked by physical study.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static KNOWLEDGE: &[KnowledgeDefinition] = &[KnowledgeDefinition { name: "metallurgy" }];

content_handle!(KnowledgeId, KnowledgeDefinition, KNOWLEDGE, knowledge);

pub const METALLURGY: KnowledgeId = knowledge("metallurgy");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable_and_unknown_knowledge_is_rejected() {
        assert_eq!(METALLURGY.name(), "metallurgy");
        assert_eq!(KnowledgeId::from_name("metallurgy"), Some(METALLURGY));
        assert_eq!(KnowledgeId::from_name("unknown"), None);
    }
}
