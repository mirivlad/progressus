use std::process::Command;

fn headless_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_progressus-headless"))
}

#[test]
fn long_run_prints_deterministic_client_summary() {
    let output = headless_command()
        .args(["--seed", "42", "--ticks", "100000"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("seed=42 tick=100000 worldgen_version=1 chunks=3 characters=5"));
    assert!(stdout.contains(
        "character id=1 name=Ada position_subunits=(-1536, 512) containing_cell=(-2, 0)"
    ));
    assert!(
        stdout.contains(
            "character id=5 name=Elin position_subunits=(2560, 512) containing_cell=(2, 0)"
        )
    );
    assert!(stdout.contains("terrain grass="));
    assert!(stdout.contains(" water="));
    assert!(stdout.contains(" rock="));
}

#[test]
fn missing_tick_count_is_rejected() {
    let output = headless_command().args(["--seed", "42"]).output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains(
        "usage: progressus-headless --seed <u64> (--ticks <u64> | --travel-chunks <positive u64>)"
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
    assert!(stdout.contains("steps=5050"));
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
