#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::Read;
use std::process::{Command, Output, Stdio};

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

fn assert_broken_pipe_exits_cleanly(args: &[&str]) {
    let mut child = chordsmith()
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chordsmith");
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let mut byte = [0u8; 1];
    stdout.read_exact(&mut byte).expect("read first byte");
    drop(stdout);

    let output = child.wait_with_output().expect("wait for chordsmith");
    assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
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
    assert!(!stdout.contains("Bbdim9no3"));
    assert!(!stdout.contains("C/A#"));
}

#[test]
fn identify_accepts_alternate_six_string_tuning() {
    let output = chordsmith()
        .args(["identify", "--tuning", "DADGAD", "000000"])
        .output()
        .expect("run chordsmith identify --tuning DADGAD");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notes: D A D G A D"));
    assert!(stdout.contains("Primary: Dsus4"));
}

#[test]
fn identify_accepts_extended_range_guitar_tuning() {
    let output = chordsmith()
        .args(["identify", "--instrument", "guitar7", "0000000"])
        .output()
        .expect("run chordsmith identify guitar7");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notes: B E A D G B E"));

    let output = chordsmith()
        .args(["identify", "--tuning", "F#BEADGBE", "00000000"])
        .output()
        .expect("run chordsmith identify 8-string tuning");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Input: 00000000"));
    assert!(stdout.contains("Notes: F#"));
}

#[test]
fn identify_accepts_ukulele_instrument() {
    let output = chordsmith()
        .args(["identify", "--instrument", "ukulele", "2010"])
        .output()
        .expect("run chordsmith identify --instrument ukulele");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notes: A C F A"));
    assert!(stdout.contains("Primary: F"));
}

#[test]
fn identify_accepts_explicit_ukulele_octave_tuning() {
    let output = chordsmith()
        .args([
            "identify",
            "--instrument",
            "ukulele",
            "--tuning",
            "G3,C4,E4,A4",
            "0000",
        ])
        .output()
        .expect("run chordsmith identify low-G ukulele");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Primary: C6/G"));
}

#[test]
fn voicings_accepts_ukulele_instrument() {
    let output = chordsmith()
        .args([
            "voicings",
            "--json",
            "--instrument",
            "ukulele",
            "--limit",
            "1",
            "C",
        ])
        .output()
        .expect("run chordsmith voicings --instrument ukulele");

    assert_success(&output);
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid ukulele voicings json");
    let first = &value.as_array().expect("voicings array")[0];
    assert_eq!(first["compact"], "0003");
    assert_eq!(first["frets"].as_array().expect("frets array").len(), 4);
}

#[test]
fn identify_accepts_unicode_tuning_glyphs() {
    let output = chordsmith()
        .args(["identify", "--tuning", "F♯,B,E,A,C♯,F♯", "000000"])
        .output()
        .expect("run chordsmith identify --tuning with unicode accidentals");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Notes: F# B E A C# F#"));
}

#[test]
fn analyze_prints_non_chord_slash_bass() {
    let output = chordsmith()
        .args(["analyze", "C/D"])
        .output()
        .expect("run chordsmith analyze C/D");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("C/D"));
    assert!(stdout.contains("Notes: C E G D"));
    assert!(stdout.contains("Intervals: 1 3 5 bass"));
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
fn text_output_treats_broken_pipe_as_success() {
    assert_broken_pipe_exits_cleanly(&["voicings", "--all", "C"]);
}

#[test]
fn json_output_treats_broken_pipe_as_success() {
    assert_broken_pipe_exits_cleanly(&["voicings", "--json", "--all", "C"]);
}

#[test]
fn voicings_rejects_pathological_limit() {
    let output = chordsmith()
        .args(["voicings", "--limit", "1000000", "Calt"])
        .output()
        .expect("run chordsmith voicings --limit 1000000 Calt");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid limit"));
}

#[test]
fn unknown_chord_descriptor_exits_as_data_error() {
    let output = chordsmith()
        .args(["analyze", "Cwat"])
        .output()
        .expect("run chordsmith analyze Cwat");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("chord descriptor"));
}

#[test]
fn malformed_numbered_descriptor_exits_as_data_error() {
    let output = chordsmith()
        .args(["analyze", "C79"])
        .output()
        .expect("run chordsmith analyze C79");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("chord descriptor"));
}

#[test]
fn malformed_parenthesized_descriptor_exits_as_data_error() {
    let output = chordsmith()
        .args(["analyze", "C(ma)j7"])
        .output()
        .expect("run chordsmith analyze C(ma)j7");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("chord descriptor"));
}

#[test]
fn leading_alteration_canonical_name_is_grouped() {
    let output = chordsmith()
        .args(["analyze", "C(b9)"])
        .output()
        .expect("run chordsmith analyze C(b9)");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.lines().next().is_some_and(|line| line == "C(b9)"));
    assert!(stdout.contains("Notes: C E G Db"));
}

#[test]
fn invalid_alt_descriptor_exits_as_data_error() {
    let output = chordsmith()
        .args(["analyze", "Cmalt"])
        .output()
        .expect("run chordsmith analyze Cmalt");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("alt is already"));
}

#[test]
fn malformed_slash_chord_exits_as_data_error() {
    let output = chordsmith()
        .args(["analyze", "C/9"])
        .output()
        .expect("run chordsmith analyze C/9");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("slash chord"));
}

#[test]
fn clap_errors_are_not_wrapped_twice() {
    let output = chordsmith()
        .args(["identify"])
        .output()
        .expect("run chordsmith identify without argument");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required"));
    assert!(!stderr.contains("clap exited"));
}

#[test]
fn voicings_rejects_frets_outside_standard_range() {
    let output = chordsmith()
        .args(["voicings", "--min-fret", "25", "C"])
        .output()
        .expect("run chordsmith voicings --min-fret 25");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("standard guitar range"));

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

    let output = chordsmith()
        .args(["voicings", "--min-fret", "13", "--max-fret", "12", "C"])
        .output()
        .expect("run chordsmith voicings --min-fret 13 --max-fret 12");

    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("min_fret"));
}

#[test]
fn voicings_min_fret_filters_low_and_open_frets() {
    let output = chordsmith()
        .args([
            "voicings",
            "--json",
            "--min-fret",
            "12",
            "--max-fret",
            "15",
            "C",
        ])
        .output()
        .expect("run chordsmith voicings --min-fret 12 --max-fret 15");

    assert_success(&output);
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid voicings json");
    let shapes = value.as_array().expect("voicings array");
    assert!(!shapes.is_empty());
    for shape in shapes {
        let frets = shape["frets"].as_array().expect("frets array");
        let played = frets
            .iter()
            .filter_map(|fret| fret.as_u64())
            .collect::<Vec<_>>();
        assert!(!played.is_empty(), "{shape}");
        assert!(
            played.iter().all(|fret| (12..=15).contains(fret)),
            "{shape}"
        );
    }

    let output = chordsmith()
        .args([
            "voicings",
            "--json",
            "--min-fret",
            "12",
            "--limit",
            "5",
            "C",
        ])
        .output()
        .expect("run chordsmith voicings --min-fret 12");

    assert_success(&output);
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid voicings json");
    let shapes = value.as_array().expect("voicings array");
    assert!(!shapes.is_empty());
    for shape in shapes {
        let frets = shape["frets"].as_array().expect("frets array");
        let played = frets
            .iter()
            .filter_map(|fret| fret.as_u64())
            .collect::<Vec<_>>();
        assert!(!played.is_empty(), "{shape}");
        assert!(played.iter().all(|fret| *fret >= 12), "{shape}");
    }
}
