//! Long-running Prototype 02 acceptance on the authoritative simulation only.

use super::*;
use crate::simulation::test_support::{
    insert_ground_stack, place_clear_workbench, total_item_quantity,
};
use progressus_content::{item, knowledge, recipe, skill, structure, workstation};

#[test]
fn settlement_studies_metal_smelts_and_sustains_five_people_for_ten_thousand_ticks() {
    let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
    let bed_cell = WorldCell::new(-4, -4);
    simulation
        .designate_construction(structure::BED, bed_cell)
        .unwrap();
    let workbench = place_clear_workbench(&mut simulation);
    let furnace_cell = (-5..=5)
        .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
        .find(|cell| {
            simulation.validate_workstation_cell(*cell).is_ok()
                && simulation
                    .construction_cell_has_removable_occupant(*cell)
                    .is_ok_and(|occupied| !occupied)
                && simulation.default_production_ports(*cell).is_ok()
        })
        .unwrap();
    let furnace = simulation
        .place_workstation(workstation::FURNACE, furnace_cell)
        .unwrap();

    let study_input = simulation
        .production_logistics_world
        .get(workbench)
        .unwrap()
        .cells(ProductionZoneKind::Input)
        .next()
        .unwrap();
    let furnace_inputs = simulation
        .production_logistics_world
        .get(furnace)
        .unwrap()
        .cells(ProductionZoneKind::Input)
        .collect::<Vec<_>>();
    insert_ground_stack(&mut simulation, item::COPPER_ORE, 1, study_input);
    insert_ground_stack(&mut simulation, item::COPPER_ORE, 2, furnace_inputs[0]);
    insert_ground_stack(&mut simulation, item::WOOD, 1, furnace_inputs[1]);
    simulation
        .designate_craft(workbench, recipe::STUDY_METALLURGY)
        .unwrap();

    let mut saw_study = false;
    let mut saw_smelting = false;
    let mut saw_eating = false;
    let mut saw_sleep = false;
    let mut saw_bed_sleep = false;
    let mut saved_during_sleep = false;
    let mut max_resident = 0;
    let mut min_satiety = crate::MAX_SATIETY;
    let mut min_rest = crate::MAX_REST;
    for _ in 0..10_000 {
        simulation.advance_ticks(1).unwrap();
        if simulation.knows(knowledge::METALLURGY) && !saw_study {
            saw_study = true;
            assert_eq!(total_item_quantity(&simulation, item::COPPER_ORE), 3);
            simulation
                .designate_craft(furnace, recipe::COPPER_INGOT)
                .unwrap();
        }
        saw_smelting |= total_item_quantity(&simulation, item::COPPER_INGOT) == 1;
        saw_eating |= simulation
            .jobs()
            .any(|job| matches!(job.kind(), JobKind::Eat { .. }));
        let sleeping = simulation
            .jobs()
            .any(|job| matches!(job.kind(), JobKind::Sleep { .. }));
        saw_sleep |= sleeping;
        saw_bed_sleep |= simulation.jobs().any(|job| {
            matches!(
                job.kind(),
                JobKind::Sleep {
                    bed_id: Some(_),
                    ..
                }
            )
        });
        if sleeping && !saved_during_sleep {
            let encoded = simulation.save_json().unwrap();
            simulation = Simulation::load_json(&encoded).unwrap();
            assert_eq!(simulation.save_json().unwrap(), encoded);
            assert!(
                simulation
                    .jobs()
                    .any(|job| matches!(job.kind(), JobKind::Sleep { .. }))
            );
            saved_during_sleep = true;
        }
        max_resident = max_resident.max(simulation.resident_chunk_count());
        for character in simulation.characters() {
            min_satiety = min_satiety.min(character.satiety());
            min_rest = min_rest.min(character.rest());
            assert!(character.satiety() > 0);
            assert!(character.rest() > 0);
        }
        assert!(simulation.item_world.indexes_are_consistent());
        assert!(simulation.job_world.indexes_are_consistent());
    }

    assert!(
        saw_study && saw_smelting && saw_eating && saw_sleep && saw_bed_sleep && saved_during_sleep
    );
    assert!(
        simulation
            .construction_world
            .structure_at(bed_cell)
            .is_some()
    );
    assert_eq!(total_item_quantity(&simulation, item::COPPER_INGOT), 1);
    assert_eq!(total_item_quantity(&simulation, item::COPPER_ORE), 1);
    assert_eq!(total_item_quantity(&simulation, item::WOOD), 16);
    assert_eq!(total_item_quantity(&simulation, item::STONE), 14);
    assert!(
        simulation
            .characters()
            .any(|character| character.skill_practice(skill::CRAFTING) > 0)
    );
    assert!(max_resident <= crate::RESIDENT_CHUNKS_PER_CENTER * 5);
    let encoded = simulation.save_json().unwrap();
    assert_eq!(
        Simulation::load_json(&encoded)
            .unwrap()
            .save_json()
            .unwrap(),
        encoded
    );
    eprintln!(
        "prototype02 seed=0 ticks=10000 max_resident={max_resident} min_satiety={min_satiety} min_rest={min_rest} study={saw_study} smelt={saw_smelting} sleep_reload={saved_during_sleep}"
    );
}
