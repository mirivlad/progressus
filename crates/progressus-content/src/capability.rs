//! What an equipped item lets its bearer do.
//!
//! Requirements name a capability rather than a specific item, so a better
//! tool satisfies every requirement the old one did without a single edit
//! elsewhere. A skill will later satisfy an entry in the same list. See
//! ADR-0024.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
}

/// Append-only. Only capabilities something actually requires belong here: one
/// that nothing reads is a placeholder, not a design.
pub static CAPABILITIES: &[CapabilityDefinition] = &[CapabilityDefinition { name: "mine" }];

content_handle!(CapabilityId, CapabilityDefinition, CAPABILITIES, capability);

pub const MINE: CapabilityId = capability("mine");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        assert_eq!(MINE.name(), "mine");
        assert_eq!(CapabilityId::from_name("mine"), Some(MINE));
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(CapabilityId::from_name("smelt"), None);
    }
}
