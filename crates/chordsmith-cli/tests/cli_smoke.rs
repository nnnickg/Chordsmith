#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::process::{Command, Output};

fn chordsmith() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chordsmith"))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn version_flag_exits_successfully() {
    let output = chordsmith()
        .arg("--version")
        .output()
        .expect("run chordsmith --version");

    assert_success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        concat!("chordsmith ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn identify_prints_primary_chord() {
    let output = chordsmith()
        .args(["identify", "022000"])
        .output()
        .expect("run chordsmith identify");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Primary: Em"));
}

#[test]
fn identify_json_outputs_primary_symbol() {
    let output = chordsmith()
        .args(["identify", "--json", "x12010"])
        .output()
        .expect("run chordsmith identify --json");

    assert_success(&output);
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid identify json");
    assert_eq!(value["primary"]["symbol"], "C7/Bb");
}

#[test]
fn identify_hides_theoretical_aliases_in_text_output() {
    let output = chordsmith()
        .args(["identify", "x12010"])
        .output()
        .expect("run chordsmith identify");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Primary: C7/Bb"));
    assert!(!stdout.contains("Bbdim9(no3)"));
}

#[test]
fn voicings_all_returns_more_than_default_limit() {
    let limited = chordsmith()
        .args(["voicings", "--json", "C"])
        .output()
        .expect("run chordsmith voicings --json");
    assert_success(&limited);
    let limited: serde_json::Value =
        serde_json::from_slice(&limited.stdout).expect("valid limited voicings json");

    let all = chordsmith()
        .args(["voicings", "--json", "--all", "C"])
        .output()
        .expect("run chordsmith voicings --json --all");
    assert_success(&all);
    let all: serde_json::Value =
        serde_json::from_slice(&all.stdout).expect("valid all voicings json");

    assert_eq!(limited.as_array().expect("limited array").len(), 15);
    assert!(all.as_array().expect("all array").len() > 15);
}

#[test]
fn voicings_all_rejects_limit() {
    let output = chordsmith()
        .args(["voicings", "--all", "--limit", "2", "C"])
        .output()
        .expect("run chordsmith voicings --all --limit");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--all cannot be used with --limit"));
}

#[test]
fn voicings_rejects_frets_outside_standard_range() {
    let output = chordsmith()
        .args(["voicings", "--max-fret", "25", "C"])
        .output()
        .expect("run chordsmith voicings --max-fret 25");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("standard guitar range"));

    let output = chordsmith()
        .args(["voicings", "--max-span", "25", "C"])
        .output()
        .expect("run chordsmith voicings --max-span 25");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("standard guitar range"));
}
