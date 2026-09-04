use std::process::Command;

fn headless_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_progressus-headless"))
}

fn numeric_field(stdout: &str, prefix: &str) -> u64 {
    stdout
        .split_whitespace()
        .find_map(|field| field.strip_prefix(prefix))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("missing numeric field {prefix}"))
}

#[test]
fn long_run_prints_deterministic_client_summary() {
    let output = headless_command()
        .args(["--seed", "42", "--ticks", "100000"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(
        "seed=42 tick=100000 worldgen_version=2 chunks=2 characters=5 resident_chunks=12"
    ));
    assert!(
        stdout.contains(
            "character id=1 name=Ada position_subunits=(512, -512) containing_cell=(0, -1)"
        )
    );
    assert!(stdout.contains(
        "character id=5 name=Elin position_subunits=(-512, -1536) containing_cell=(-1, -2)"
    ));
    assert!(stdout.contains("terrain grass="));
    assert!(stdout.contains(" water="));
    assert!(stdout.contains(" rock="));
    assert!(stdout.contains(" unknown="));
}

#[test]
fn missing_tick_count_is_rejected() {
    let output = headless_command().args(["--seed", "42"]).output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains(
        "usage: progressus-headless --seed <u64> (--ticks <u64> | --travel-chunks <positive u64> | --activity-smoke)"
    ));
}

#[test]
fn invalid_seed_is_rejected() {
    let output = headless_command()
        .args(["--seed", "invalid", "--ticks", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid seed 'invalid'")
    );
}

#[test]
fn unknown_arguments_are_rejected() {
    let output = headless_command()
        .args(["--seed", "42", "--ticks", "1", "--wat", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown argument '--wat'")
    );
}

#[test]
fn duplicate_arguments_are_rejected() {
    let output = headless_command()
        .args(["--seed", "42", "--seed", "73", "--ticks", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("duplicate --seed argument")
    );
}

#[test]
fn travel_chunks_crosses_many_boundaries_deterministically() {
    let first = headless_command()
        .args(["--seed", "0", "--travel-chunks", "64"])
        .output()
        .unwrap();
    let second = headless_command()
        .args(["--seed", "0", "--travel-chunks", "64"])
        .output()
        .unwrap();

    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let stdout = String::from_utf8(first.stdout).unwrap();
    assert!(stdout.contains("travel character_id=3"));
    assert!(stdout.contains("crossed_boundaries=64"));
    assert!(stdout.contains("steps=2239"));
    assert!(stdout.contains("resident_chunks="));
    assert!(stdout.contains("max_resident_chunks="));
}

#[test]
fn activity_smoke_exercises_physical_work_save_load_and_bounded_residency() {
    let output = headless_command()
        .args(["--seed", "0", "--activity-smoke"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("activity seed=0 tick=100000"));
    assert!(stdout.contains("designated_resources=28"));
    assert!(stdout.contains("tools=13"));
    assert!(stdout.contains("structures=3"));
    assert!(stdout.contains("save_reload_while_carrying=true"));
    let resident = numeric_field(&stdout, "resident_chunks=");
    let max_resident = numeric_field(&stdout, "max_resident_chunks=");
    assert!(
        resident <= 45,
        "five radius-one character neighborhoods must stay bounded"
    );
    assert!(
        max_resident <= 45,
        "activity smoke exceeded bounded raw residency"
    );
}

#[test]
fn activity_smoke_is_mutually_exclusive_with_other_scenarios() {
    let output = headless_command()
        .args(["--seed", "0", "--activity-smoke", "--ticks", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("choose exactly one of --ticks, --travel-chunks, or --activity-smoke")
    );
}

#[test]
fn travel_chunks_requires_a_positive_count() {
    let output = headless_command()
        .args(["--seed", "0", "--travel-chunks", "0"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("travel chunk count must be positive")
    );
}
