//! Where a character may equip something.
//!
//! No system may match on a specific slot. Simulation code asks whether a
//! character has something equipped that provides a required capability, never
//! whether a named slot holds a named item. That rule is what keeps adding a
//! slot to one registry row plus a localized name; if a new slot ever requires
//! editing a system, ADR-0024 has been violated.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
}

/// Append-only. Weapon and clothing are deliberately absent: combat is not in
/// this milestone and clothing without temperature or protection is
/// decoration, so neither would be read by anything.
pub static SLOTS: &[SlotDefinition] = &[SlotDefinition { name: "tool" }];

content_handle!(SlotId, SlotDefinition, SLOTS, slot);

pub const TOOL: SlotId = slot("tool");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        assert_eq!(TOOL.name(), "tool");
        assert_eq!(SlotId::from_name("tool"), Some(TOOL));
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(SlotId::from_name("weapon"), None);
        assert_eq!(SlotId::from_name("clothing"), None);
    }
}
