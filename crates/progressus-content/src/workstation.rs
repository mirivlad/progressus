//! Production buildings that host recipes.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkstationDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    /// How many physical material-input ports the building owns.
    pub input_ports: usize,
    /// How many physical product-output ports the building owns.
    pub output_ports: usize,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static WORKSTATIONS: &[WorkstationDefinition] = &[WorkstationDefinition {
    name: "workbench",
    input_ports: 2,
    output_ports: 2,
}];

content_handle!(
    WorkstationId,
    WorkstationDefinition,
    WORKSTATIONS,
    workstation
);

pub const WORKBENCH: WorkstationId = workstation("workbench");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        assert_eq!(WORKBENCH.name(), "workbench");
        assert_eq!(WorkstationId::from_name("workbench"), Some(WORKBENCH));
    }

    #[test]
    fn every_workstation_can_receive_material_and_emit_product() {
        for id in WorkstationId::all() {
            let definition = id.definition();
            assert!(
                definition.input_ports > 0,
                "{} can never be supplied",
                id.name()
            );
            assert!(
                definition.output_ports > 0,
                "{} can never deliver a product",
                id.name()
            );
        }
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(WorkstationId::from_name("furnace"), None);
    }
}
