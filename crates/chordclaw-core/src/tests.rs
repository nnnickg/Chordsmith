#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::identify::InferredOmission;
use crate::inline_vec::InlineVec;
use crate::scoring::rank_diverse_voicing_candidates;
use crate::symbol::MAX_OMISSIONS;
use crate::voicing::VoicingCandidate;

fn primary(input: &str) -> String {
    identify(input)
        .unwrap()
        .primary
        .expect("primary analysis")
        .symbol
}

fn candidates_from_voicings(voicings: &[Voicing]) -> Vec<VoicingCandidate> {
    voicings
        .iter()
        .map(|voicing| VoicingCandidate {
            frets: {
                let mut frets = [None; MAX_STRING_COUNT];
                for (idx, fret) in voicing.frets.iter().copied().enumerate() {
                    frets[idx] = fret;
                }
                frets
            },
            string_count: voicing.frets.len(),
            omissions: omission_degrees(&voicing.omissions),
            score: voicing.score,
        })
        .collect()
}

fn omission_degrees(omissions: &[String]) -> InlineVec<InferredOmission, MAX_OMISSIONS> {
    let mut out = InlineVec::default();
    for omission in omissions {
        let omission = match omission.as_str() {
            "1" => InferredOmission::Root,
            "3" => InferredOmission::Third,
            "5" => InferredOmission::Fifth,
            other => panic!("unexpected omission degree {other}"),
        };
        let _ = out.push(omission);
    }
    out
}

fn identifies_as_primary_or_useful_alias(result: &IdentifyResult, requested: &str) -> bool {
    result
        .primary
        .as_ref()
        .is_some_and(|analysis| symbol_matches_requested(&analysis.symbol, requested))
        || result.aliases.iter().any(|analysis| {
            symbol_matches_requested(&analysis.symbol, requested)
                && analysis.class == AnalysisClass::UsefulAlias
        })
}

fn symbol_matches_requested(actual: &str, requested: &str) -> bool {
    if actual == requested
        || actual
            .strip_prefix(requested)
            .is_some_and(|rest| rest.starts_with("(no") || rest.starts_with("no"))
    {
        return true;
    }

    let Some((requested_root, requested_bass)) = requested.split_once('/') else {
        return false;
    };
    actual
        .strip_prefix(requested_root)
        .and_then(|rest| {
            if rest.starts_with("(no") || rest.starts_with("no") {
                rest.rsplit_once('/')
            } else {
                None
            }
        })
        .is_some_and(|(_, actual_bass)| actual_bass == requested_bass)
}

#[test]
fn canonical_symbol_names_round_trip_for_representative_corpus() {
    for input in [
        "C", "Cm", "Cmaj7", "C7sus4", "C6/9", "C(b9)", "Db(b9)", "C7b9#9", "Cø7no3", "C/G", "C/D",
        "F#dim7", "Bbmaj9", "Gsus4", "Aadd9",
    ] {
        let (symbol, formula) = analyze_symbol(input).unwrap();
        let canonical = symbol.name();
        let (round_trip, round_trip_formula) = analyze_symbol(&canonical).unwrap();
        assert_eq!(round_trip.name(), canonical, "{input}");
        assert_eq!(round_trip_formula, formula, "{input}");
    }
}

#[test]
fn top_generated_voicing_identifies_as_requested_or_useful_alias() {
    for input in [
        "C", "Cm", "G", "Em", "C/G", "C/D", "C7", "Cmaj7", "C7sus4", "F#dim7", "Bbmaj7", "Dsus4",
    ] {
        let requested = analyze_symbol(input).unwrap().0.name();
        let shape = voicings(input, VoicingOptions::default())
            .unwrap()
            .into_iter()
            .next()
            .expect("top generated voicing");
        let result = identify(&shape.compact).unwrap();
        assert!(
            identifies_as_primary_or_useful_alias(&result, &requested),
            "{input} requested {requested}, generated {}, primary {:?}, aliases {:?}",
            shape.compact,
            result.primary,
            result.aliases
        );
    }
}

#[test]
fn identifies_basic_open_chords() {
    assert_eq!(primary("022000"), "Em");
    assert_eq!(primary("x32010"), "C");
    assert_eq!(primary("320003"), "G");
    assert_eq!(primary("x02220"), "A");
    assert_eq!(primary("xx0232"), "D");
}

#[test]
fn supports_six_string_alternate_tunings() {
    let dadgad = GuitarTuning::parse("DADGAD").unwrap();
    assert_eq!(
        GuitarTuning::parse("D,A,D,G,A,D").unwrap().notes(),
        dadgad.notes()
    );

    let result = identify_with_tuning("000000", dadgad).unwrap();
    let notes = result
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["D", "A", "D", "G", "A", "D"]);
    assert_eq!(result.primary.expect("primary analysis").symbol, "Dsus4");

    let shapes = voicings_with_tuning(
        "Dsus4",
        dadgad,
        VoicingOptions {
            mode: VoicingMode::Curated { limit: 5 },
            ..VoicingOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        shapes.first().map(|shape| shape.compact.as_str()),
        Some("000000")
    );
}

#[test]
fn supports_extended_range_guitar_tunings() {
    let seven = Tuning::parse("BEADGBE").unwrap();
    assert_eq!(seven.instrument(), Instrument::Guitar7);
    assert_eq!(seven.string_count(), GUITAR7_STRING_COUNT);
    let seven_open = identify_with_tuning("0000000", seven).unwrap();
    let seven_notes = seven_open
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(seven_notes, ["B", "E", "A", "D", "G", "B", "E"]);

    let eight = Tuning::parse("F#BEADGBE").unwrap();
    assert_eq!(eight.instrument(), Instrument::Guitar8);
    assert_eq!(eight.string_count(), GUITAR8_STRING_COUNT);
    let eight_open = identify_with_tuning("00000000", eight).unwrap();
    let eight_pitches = eight_open
        .notes
        .iter()
        .map(|note| note.pitch_class)
        .collect::<Vec<_>>();
    assert_eq!(eight_pitches, [6, 11, 4, 9, 2, 7, 11, 4]);

    let shapes = voicings_with_tuning(
        "E",
        Instrument::Guitar7.default_tuning(),
        VoicingOptions {
            mode: VoicingMode::Curated { limit: 1 },
            ..VoicingOptions::default()
        },
    )
    .unwrap();
    assert_eq!(shapes.first().expect("7-string voicing").frets.len(), 7);

    let shapes = voicings_with_tuning(
        "E",
        Instrument::Guitar8.default_tuning(),
        VoicingOptions {
            mode: VoicingMode::Curated { limit: 1 },
            ..VoicingOptions::default()
        },
    )
    .unwrap();
    assert_eq!(shapes.first().expect("8-string voicing").frets.len(), 8);
}

#[test]
fn maps_instruments_from_supported_string_counts() {
    assert_eq!(
        Instrument::from_string_count(4).unwrap(),
        Instrument::Ukulele
    );
    assert_eq!(
        Instrument::from_string_count(6).unwrap(),
        Instrument::Guitar
    );
    assert_eq!(
        Instrument::from_string_count(7).unwrap(),
        Instrument::Guitar7
    );
    assert_eq!(
        Instrument::from_string_count(8).unwrap(),
        Instrument::Guitar8
    );
}

#[test]
fn counts_fingering_strings_without_parsing_frets() {
    assert_eq!(Fingering::string_count_from_input("2010").unwrap(), 4);
    assert_eq!(Fingering::string_count_from_input("x-0-10-3").unwrap(), 4);
    assert_eq!(Fingering::string_count_from_input("0000000").unwrap(), 7);

    let error =
        Fingering::string_count_from_input("00000").expect_err("five-string fingering should fail");
    assert!(
        error
            .to_string()
            .contains("expected 4, 6, 7, or 8 strings, got 5"),
        "{error}"
    );
}

#[test]
fn rejects_unsupported_compact_tuning_count_after_octaves() {
    let error = Tuning::parse("C4C4C4C4C4C4C4C4C4")
        .expect_err("nine-string compact tuning should fail by string count");
    assert!(error.to_string().contains("got 9"), "{error}");
}

#[test]
fn rejects_fingering_count_mismatches_for_fixed_tuning() {
    let error = identify_with_tuning("0000000", STANDARD_TUNING)
        .expect_err("seven-string fingering should fail for six-string tuning");
    assert!(
        error.to_string().contains("expected 6 strings, got 7"),
        "{error}"
    );
}

#[test]
fn supports_standard_ukulele_tuning_and_reentrant_bass() {
    let uke = Instrument::Ukulele.default_tuning();
    let notes = uke
        .notes()
        .iter()
        .map(|note| note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["G", "C", "E", "A"]);

    let c = identify_with_tuning("0003", uke).unwrap();
    assert_eq!(c.primary.expect("C primary").symbol, "C");

    let f = identify_with_tuning("2010", uke).unwrap();
    assert_eq!(f.primary.expect("F primary").symbol, "F");

    let open = identify_with_tuning("0000", uke).unwrap();
    assert_eq!(open.primary.expect("open uke primary").symbol, "C6");
}

#[test]
fn supports_explicit_ukulele_tuning_octaves() {
    let high_g = Tuning::parse_for_instrument("G4,C4,E4,A4", Instrument::Ukulele).unwrap();
    assert_eq!(
        identify_with_tuning("0000", high_g)
            .unwrap()
            .primary
            .expect("high-G primary")
            .symbol,
        "C6"
    );

    let low_g = Tuning::parse_for_instrument("G3,C4,E4,A4", Instrument::Ukulele).unwrap();
    assert_eq!(
        identify_with_tuning("0000", low_g)
            .unwrap()
            .primary
            .expect("low-G primary")
            .symbol,
        "C6/G"
    );

    let transposed_high_g =
        Tuning::parse_for_instrument("Ab,Db,F,Bb", Instrument::Ukulele).unwrap();
    assert_eq!(
        identify_with_tuning("0000", transposed_high_g)
            .unwrap()
            .primary
            .expect("transposed high-G primary")
            .symbol,
        "Db6"
    );

    let baritone_low_d = Tuning::parse_for_instrument("DGBE", Instrument::Ukulele).unwrap();
    assert_eq!(
        identify_with_tuning("0000", baritone_low_d)
            .unwrap()
            .primary
            .expect("linear baritone primary")
            .symbol,
        "G6/D"
    );

    let baritone_high_d = Tuning::parse_for_instrument("D4,G3,B3,E4", Instrument::Ukulele).unwrap();
    assert_eq!(
        identify_with_tuning("0000", baritone_high_d)
            .unwrap()
            .primary
            .expect("high-D baritone primary")
            .symbol,
        "G6"
    );

    let mixed = Tuning::parse_for_instrument("G4,C,E,A", Instrument::Ukulele)
        .expect_err("mixed tuning octaves should fail");
    assert!(mixed.to_string().contains("octaves"));
}

#[test]
fn ranks_common_ukulele_open_shapes_first() {
    let uke = Instrument::Ukulele.default_tuning();
    for (symbol, expected) in [("C", "0003"), ("Am", "2000"), ("F", "2010"), ("G", "0232")] {
        let shapes = voicings_with_tuning(symbol, uke, VoicingOptions::default()).unwrap();
        let first = shapes.first().expect("top ukulele voicing");
        assert_eq!(first.compact, expected, "{symbol}");
        assert_eq!(first.frets.len(), UKULELE_STRING_COUNT);
    }
}

#[test]
fn ranks_ukulele_e_major_low_grips_before_high_positions() {
    let uke = Instrument::Ukulele.default_tuning();
    let shapes = voicings_with_tuning("E", uke, VoicingOptions::default()).unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        &compact[..10],
        [
            "x442", "4442", "1x02", "1402", "1442", "444x", "4447", "14x2", "x877", "9877",
        ]
    );
}

#[test]
fn golden_guitar_default_voicings_start_with_musician_grips() {
    for (symbol, expected) in [
        ("C", "x32010"),
        ("G", "320003"),
        ("D", "xx0232"),
        ("A7", "x02020"),
        ("Cadd9", "x32030"),
        ("Dadd9", "x54230"),
        ("C9", "x32330"),
        ("C7b9", "x32320"),
    ] {
        let shapes = voicings(symbol, VoicingOptions::default()).unwrap();
        let first = shapes.first().expect("top guitar voicing");
        assert_eq!(first.compact, expected, "{symbol}");
    }
}

#[test]
fn golden_ukulele_default_voicings_start_with_musician_grips() {
    let uke = Instrument::Ukulele.default_tuning();
    for (symbol, expected) in [
        ("C", "0003"),
        ("G", "0232"),
        ("A", "2100"),
        ("E", "x442"),
        ("F", "2010"),
        ("Em", "0432"),
    ] {
        let shapes = voicings_with_tuning(symbol, uke, VoicingOptions::default()).unwrap();
        let first = shapes.first().expect("top ukulele voicing");
        assert_eq!(first.compact, expected, "{symbol}");
    }
}

#[test]
fn golden_extended_dominant_shapes_identify_as_requested_family() {
    assert_eq!(primary("x32330"), "C9");
    assert_eq!(primary("878788"), "C9");
    assert_eq!(primary("x32323"), "C7b9");
    assert_eq!(primary("x6466x"), "Ebm9");
}

#[test]
fn golden_ukulele_context_voicings_remain_useful_aliases() {
    let uke = Instrument::Ukulele.default_tuning();

    assert_eq!(
        identify_with_tuning("3203", uke)
            .unwrap()
            .primary
            .expect("C9 uke primary")
            .symbol,
        "C9"
    );

    let rootless_c13 = identify_with_tuning("2201", uke).unwrap();
    assert_eq!(
        rootless_c13.primary.expect("rootless C13 primary").symbol,
        "E7sus4b5"
    );
    assert!(rootless_c13.aliases.iter().any(|analysis| {
        analysis.symbol == "C13no1"
            && analysis.omissions == ["1", "5"]
            && analysis.class == AnalysisClass::UsefulAlias
    }));

    let c_over_g = identify_with_tuning("5787", uke).unwrap();
    assert_eq!(c_over_g.primary.expect("C/G primary").symbol, "C");
    assert!(c_over_g.aliases.iter().any(|analysis| {
        analysis.symbol == "C/G" && analysis.class == AnalysisClass::UsefulAlias
    }));

    let c_over_d = identify_with_tuning("x203", uke).unwrap();
    let c_over_d_primary = c_over_d.primary.expect("C/D omitted-fifth primary");
    assert_eq!(c_over_d_primary.symbol, "C/D");
    assert_eq!(c_over_d_primary.omissions, ["5"]);

    let c_over_d_shapes = voicings_with_tuning("C/D", uke, VoicingOptions::default()).unwrap();
    assert!(c_over_d_shapes.iter().all(|shape| {
        !shape
            .omissions
            .iter()
            .any(|omission| omission.as_str() == "3")
    }));

    let rootless_g9 = identify_with_tuning("2x12", uke).unwrap();
    assert!(rootless_g9.aliases.iter().any(|analysis| {
        analysis.symbol == "G9no1"
            && analysis.omissions == ["1", "5"]
            && analysis.class == AnalysisClass::UsefulAlias
    }));

    let half_diminished = identify_with_tuning("2000", uke).unwrap();
    assert_eq!(
        half_diminished
            .primary
            .expect("rootless half-diminished primary")
            .symbol,
        "Am"
    );
    assert!(half_diminished.aliases.iter().any(|analysis| {
        analysis.symbol == "F#m7b5no1" && analysis.class == AnalysisClass::UsefulAlias
    }));

    let altered = identify_with_tuning("3344", uke).unwrap();
    assert!(altered.aliases.iter().any(|analysis| {
        analysis.symbol == "Caltno1no3" && analysis.class == AnalysisClass::UsefulAlias
    }));

    let diminished = identify_with_tuning("3101", uke).unwrap();
    assert!(diminished.aliases.iter().any(|analysis| {
        analysis.symbol == "Gdim7no1" && analysis.class == AnalysisClass::UsefulAlias
    }));
}

#[test]
fn golden_top_five_voicings_round_trip_for_guitar_and_ukulele_corpus() {
    let guitar = Instrument::Guitar.default_tuning();
    let guitar7 = Instrument::Guitar7.default_tuning();
    let guitar8 = Instrument::Guitar8.default_tuning();
    let uke = Instrument::Ukulele.default_tuning();
    let guitar_corpus = &[
        "C", "G", "Dadd9", "A7", "C9", "C13", "G13", "C7b9", "C7#9", "C7#11", "Calt", "Bm7b5",
        "Gdim7",
    ][..];
    for (instrument, tuning, corpus) in [
        ("guitar", guitar, guitar_corpus),
        ("guitar7", guitar7, guitar_corpus),
        ("guitar8", guitar8, guitar_corpus),
        (
            "ukulele",
            uke,
            &[
                "C", "G", "E", "Em", "Cadd9", "Dadd9", "C/G", "C/E", "F/A", "C/D", "C/Db", "C/Eb",
                "C/Ab", "F#m7b5", "C9", "G9", "C13", "C7#9", "C7#11", "Calt", "Gdim7",
            ][..],
        ),
    ] {
        for symbol in corpus {
            let requested = analyze_symbol(symbol).unwrap().0.name();
            let shapes = voicings_with_tuning(
                symbol,
                tuning,
                VoicingOptions {
                    mode: VoicingMode::Curated { limit: 5 },
                    ..VoicingOptions::default()
                },
            )
            .unwrap();

            for shape in shapes {
                let result = identify_with_tuning(&shape.compact, tuning).unwrap();
                assert!(
                    identifies_as_primary_or_useful_alias(&result, &requested),
                    "{instrument} {symbol} requested {requested}, generated {}, primary {:?}, aliases {:?}",
                    shape.compact,
                    result.primary,
                    result.aliases
                );
            }
        }
    }
}

#[test]
fn spells_flat_seventh_bass_from_chord_context() {
    let result = identify("x12010").unwrap();
    assert_eq!(result.primary.expect("primary analysis").symbol, "C7/Bb");
    let notes = result
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["Bb", "E", "G", "C", "E"]);
}

#[test]
fn respells_played_notes_from_primary_flat_key_context() {
    let result = identify("x46664").unwrap();
    assert_eq!(result.primary.expect("primary analysis").symbol, "Db");
    let notes = result
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["Db", "Ab", "Db", "F", "Ab"]);
}

#[test]
fn prefers_common_flat_root_for_b_flat_major() {
    assert_eq!(primary("x13331"), "Bb");
}

#[test]
fn distinguishes_half_diminished_from_full_diminished() {
    let half_diminished = identify("x3434x").unwrap();
    assert_eq!(
        half_diminished.primary.expect("primary analysis").symbol,
        "Cm7b5"
    );
    let notes = half_diminished
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["C", "Gb", "Bb", "Eb"]);

    assert_eq!(primary("x3424x"), "Cdim7");
}

#[test]
fn suppresses_redundant_duplicate_pitch_aliases() {
    let result = identify("021000").unwrap();
    assert_eq!(result.primary.expect("primary analysis").symbol, "Em(maj7)");
    let aliases = result
        .aliases
        .iter()
        .map(|analysis| analysis.symbol.as_str())
        .collect::<Vec<_>>();
    assert!(!aliases.contains(&"EmM7#9"));
    assert!(!aliases.contains(&"EmM9#9"));
    assert!(!aliases.contains(&"Em(maj7)#9"));
    assert!(!aliases.contains(&"Em(maj9)#9"));
}

#[test]
fn rejects_explicit_redundant_duplicate_pitch_symbols() {
    let error = analyze_symbol("EmM7#9").expect_err("redundant symbol should fail");
    assert!(
        error.to_string().contains("redundant chord symbol"),
        "{error}"
    );
    analyze_symbol("C7#9").expect("dominant #9 is not redundant");
}

#[test]
fn public_chord_symbol_parse_rejects_redundant_states() {
    for symbol in ["EmM7#9", "Cdim7b5", "Caug#5"] {
        let error = ChordSymbol::parse(symbol).expect_err("redundant symbol should fail");
        assert!(
            error.to_string().contains("redundant chord symbol"),
            "{symbol}: {error}"
        );
    }

    let symbol = ChordSymbol::parse("C7#9").expect("valid symbol should parse");
    assert_eq!(symbol.name(), "C7#9");
    let formula = symbol.formula();
    let intervals = formula
        .tones()
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "3", "5", "b7", "#9"]);
}

#[test]
fn rejects_redundant_same_degree_alterations() {
    let error = analyze_symbol("Cdim7b5").expect_err("redundant symbol should fail");
    assert!(error.to_string().contains("alteration restates"), "{error}");

    let error = analyze_symbol("Caug#5").expect_err("redundant symbol should fail");
    assert!(error.to_string().contains("alteration restates"), "{error}");

    analyze_symbol("Cm7b5").expect("minor seven flat five is not redundant");
}

#[test]
fn parses_six_nine_as_a_chord_quality_not_a_slash_bass() {
    let (symbol, formula) = analyze_symbol("C6/9").unwrap();
    assert_eq!(symbol.name(), "C6/9");
    let notes = formula
        .tones
        .iter()
        .map(|tone| tone.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["C", "E", "G", "A", "D"]);

    let (symbol, _) = analyze_symbol("C(add9)").unwrap();
    assert_eq!(symbol.name(), "Cadd9");

    let (symbol, _) = analyze_symbol("C(6/9)").unwrap();
    assert_eq!(symbol.name(), "C6/9");

    let (symbol, _) = analyze_symbol("C6/9/E").unwrap();
    assert_eq!(symbol.name(), "C6/9/E");
}

#[test]
fn rejects_malformed_slash_syntax() {
    for symbol in ["C//D", "C/9", "C7//E", "C/"] {
        let error = analyze_symbol(symbol).expect_err("malformed slash should fail");
        assert!(
            error.to_string().contains("slash chord"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn rejects_enharmonic_chord_tone_slash_bass_spellings() {
    for (symbol, expected) in [("C/Fb", "E"), ("C/B#", "C")] {
        let error = analyze_symbol(symbol).expect_err("enharmonic slash bass should fail");
        assert!(
            error.to_string().contains("slash chord bass") && error.to_string().contains(expected),
            "{symbol}: {error}"
        );
    }

    assert_eq!(analyze_symbol("C/E").unwrap().0.name(), "C/E");
    assert_eq!(analyze_symbol("C/D").unwrap().0.name(), "C/D");
}

#[test]
fn rejects_redundant_root_slash_root_symbols() {
    for (symbol, expected) in [("C/C", "C"), ("Cm/C", "Cm"), ("F#maj7/F#", "F#maj7")] {
        let error = analyze_symbol(symbol).expect_err("root slash root should fail");
        assert!(
            error.to_string().contains("bass repeats root") && error.to_string().contains(expected),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn rejects_duplicate_or_conflicting_descriptor_state() {
    for symbol in ["C79", "C97", "Cmaj7maj9", "C(add9", "C66", "Cadd9add9"] {
        let error = analyze_symbol(symbol).expect_err("malformed descriptor should fail");
        assert!(
            error.to_string().contains("chord descriptor"),
            "{symbol}: {error}"
        );
    }

    for symbol in ["C(ma)j7", "C(ad)d9", "C m a j 7"] {
        let error = analyze_symbol(symbol).expect_err("malformed descriptor should fail");
        assert!(
            error.to_string().contains("chord descriptor"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn rejects_symbols_that_lie_about_altered_extension_degrees() {
    for symbol in ["C9b9", "C11#11", "C13b13", "Cadd9b9"] {
        let error = analyze_symbol(symbol).expect_err("altered extension should fail");
        assert!(
            error.to_string().contains("chord descriptor"),
            "{symbol}: {error}"
        );
    }

    analyze_symbol("C13b9#9").expect("lower altered extensions remain valid");
}

#[test]
fn parses_major_seventh_as_major_not_dominant() {
    let (symbol, formula) = analyze_symbol("Cmaj7").unwrap();
    assert_eq!(symbol.name(), "Cmaj7");
    let notes = formula
        .tones
        .iter()
        .map(|tone| tone.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["C", "E", "G", "B"]);

    let (symbol, formula) = analyze_symbol("CM9").unwrap();
    assert_eq!(symbol.name(), "Cmaj9");
    let notes = formula
        .tones
        .iter()
        .map(|tone| tone.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["C", "E", "G", "B", "D"]);

    let (symbol, formula) = analyze_symbol("CMAJ7").unwrap();
    assert_eq!(symbol.name(), "Cmaj7");
    let notes = formula
        .tones
        .iter()
        .map(|tone| tone.note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["C", "E", "G", "B"]);
}

#[test]
fn parses_lowercase_bare_major_descriptor() {
    for (input, expected) in [
        ("Cmaj", "C"),
        ("Cmaj6", "C6"),
        ("Cmajadd9", "Cadd9"),
        ("Cmajno5", "Cno5"),
    ] {
        let (symbol, _) = analyze_symbol(input).unwrap();
        assert_eq!(symbol.name(), expected, "{input}");
    }
}

#[test]
fn parses_common_chart_symbol_synonyms() {
    let (symbol, _) = analyze_symbol("CΔ7").unwrap();
    assert_eq!(symbol.name(), "Cmaj7");

    let (symbol, _) = analyze_symbol("C△9").unwrap();
    assert_eq!(symbol.name(), "Cmaj9");

    let (symbol, formula) = analyze_symbol("C°7").unwrap();
    assert_eq!(symbol.name(), "Cdim7");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "b3", "b5", "bb7"]);

    let (symbol, _) = analyze_symbol("Cø7").unwrap();
    assert_eq!(symbol.name(), "Cm7b5");

    for input in ["Cmin7", "CMin7", "CMIN7"] {
        let (symbol, _) = analyze_symbol(input).unwrap();
        assert_eq!(symbol.name(), "Cm7", "{input}");
    }
}

#[test]
fn parses_common_unicode_chart_glyphs() {
    for (input, canonical) in [
        ("C♭maj7", "Cbmaj7"),
        ("F♯m7", "F#m7"),
        ("C𝄫maj7", "Cbbmaj7"),
        ("F𝄪m7", "F##m7"),
        ("C7♭9", "C7b9"),
        ("C−7", "Cm7"),
        ("C–7", "Cm7"),
    ] {
        let (symbol, _) = analyze_symbol(input).unwrap();
        assert_eq!(symbol.name(), canonical, "{input}");
    }
}

#[test]
fn rejects_alterations_outside_identification_grammar() {
    for symbol in ["Csus4b11", "C7##13", "C7b13#13", "C7bb5"] {
        let error = analyze_symbol(symbol).expect_err("unsupported alteration should fail");
        assert!(
            error.to_string().contains("unsupported alteration")
                || error.to_string().contains("chord descriptor"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn parses_unicode_note_and_tuning_glyphs() {
    assert_eq!(NoteName::parse("F♯").unwrap().to_string(), "F#");
    assert_eq!(NoteName::parse("C♭").unwrap().to_string(), "Cb");
    assert_eq!(NoteName::parse("F𝄪").unwrap().to_string(), "F##");
    assert_eq!(NoteName::parse("C𝄫").unwrap().to_string(), "Cbb");

    let tuning = GuitarTuning::parse("F♯,B,E,A,C♯,F♯").unwrap();
    let notes = tuning
        .notes()
        .iter()
        .map(|note| note.to_string())
        .collect::<Vec<_>>();
    assert_eq!(notes, ["F#", "B", "E", "A", "C#", "F#"]);
}

#[test]
fn parses_half_diminished_extension_shorthand() {
    for (input, canonical, intervals) in [
        ("Cø9", "Cm9b5", vec!["1", "b3", "b5", "b7", "9"]),
        ("Cø11", "Cm11b5", vec!["1", "b3", "b5", "b7", "9", "11"]),
    ] {
        let (symbol, formula) = analyze_symbol(input).unwrap();
        assert_eq!(symbol.name(), canonical, "{input}");
        let actual = formula
            .tones()
            .iter()
            .map(|tone| tone.interval.to_string())
            .collect::<Vec<_>>();
        assert_eq!(actual, intervals, "{input}");
    }

    let error = analyze_symbol("Cø79").expect_err("ø7 plus extension should be rejected");
    assert!(error.to_string().contains("conflicting"), "{error}");
}

#[test]
fn parses_accidentals_without_overflow_or_mixed_spelling() {
    let excessive = format!("C{}", "#".repeat(128));
    let error = analyze_symbol(&excessive).expect_err("excessive accidentals should fail");
    assert!(
        error.to_string().contains("too many accidentals"),
        "{error}"
    );

    let error = analyze_symbol("C#b").expect_err("mixed spelling should fail");
    assert!(error.to_string().contains("chord descriptor"), "{error}");

    let error = analyze_symbol("Cb#").expect_err("mixed spelling should fail");
    assert!(error.to_string().contains("chord descriptor"), "{error}");

    let (symbol, formula) = analyze_symbol("C#b5").unwrap();
    assert_eq!(symbol.name(), "C#(b5)");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "3", "b5"]);
}

#[test]
fn canonical_leading_alterations_round_trip_without_root_collision() {
    for (input, canonical, notes) in [
        ("C(b9)", "C(b9)", vec!["C", "E", "G", "Db"]),
        ("C(b5)", "C(b5)", vec!["C", "E", "Gb"]),
        ("C(#5)", "C(#5)", vec!["C", "E", "G#"]),
        ("Db(b9)", "Db(b9)", vec!["Db", "F", "Ab", "Ebb"]),
    ] {
        let (symbol, formula) = analyze_symbol(input).unwrap();
        assert_eq!(symbol.name(), canonical);
        assert_eq!(
            formula
                .tones()
                .iter()
                .map(|tone| tone.note.to_string())
                .collect::<Vec<_>>(),
            notes,
            "{input}"
        );

        let (round_trip, round_trip_formula) = analyze_symbol(&symbol.name()).unwrap();
        assert_eq!(round_trip.name(), canonical);
        assert_eq!(round_trip_formula, formula);
    }

    let (flat_root, flat_root_formula) = analyze_symbol("Cb9").unwrap();
    assert_eq!(flat_root.name(), "Cb9");
    assert_eq!(
        flat_root_formula
            .tones()
            .iter()
            .map(|tone| tone.note.to_string())
            .collect::<Vec<_>>(),
        ["Cb", "Eb", "Gb", "Bbb", "Db"]
    );
}

#[test]
fn note_name_api_rejects_invalid_accidental_states() {
    let error = NoteName::new(NoteLetter::B, 3).expect_err("triple sharp should fail");
    assert!(
        error.to_string().contains("too many accidentals"),
        "{error}"
    );

    let note = NoteName::new(NoteLetter::B, 2).unwrap();
    assert_eq!(note.to_string(), "B##");
    assert_eq!(note.letter(), NoteLetter::B);
    assert_eq!(note.accidental(), 2);

    let error = analyze_symbol("B##maj7").expect_err("triple spellings should fail");
    assert!(error.to_string().contains("formula spelling"), "{error}");
}

#[test]
fn parses_suspended_chords_after_sevenths_and_extensions() {
    let (symbol, formula) = analyze_symbol("C7sus4").unwrap();
    assert_eq!(symbol.name(), "C7sus4");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "4", "5", "b7"]);

    let (symbol, formula) = analyze_symbol("C9sus4").unwrap();
    assert_eq!(symbol.name(), "C9sus4");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "4", "5", "b7", "9"]);
}

#[test]
fn rejects_conflicting_quality_rewrites() {
    for symbol in ["Cdimø7", "Cm7sus4", "C5sus4", "Cdim7sus4"] {
        let error = analyze_symbol(symbol).expect_err("conflicting quality should fail");
        assert!(
            error.to_string().contains("duplicate or conflicting"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn half_diminished_canonicalization_preserves_omissions() {
    let (symbol, formula) = analyze_symbol("Cø7no3").unwrap();
    assert_eq!(symbol.name(), "Cm7b5no3");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "b5", "b7"]);
}

#[test]
fn rejects_noop_or_empty_explicit_omissions() {
    for symbol in ["Cno1no3no5", "Csus4no3", "Cno1no3"] {
        let error = analyze_symbol(symbol).expect_err("invalid omission should fail");
        assert!(
            error.to_string().contains("omit") || error.to_string().contains("harmonic content"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn parses_alt_as_a_concrete_altered_dominant_set() {
    let (symbol, _) = analyze_symbol("Calt").unwrap();
    assert_eq!(symbol.name(), "Calt");

    let (symbol, formula) = analyze_symbol("C7alt").unwrap();
    assert_eq!(symbol.name(), "Calt");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "3", "b7", "b9", "#9", "b13"]);

    let error = analyze_symbol("Calt#9").expect_err("alt modifiers should fail");
    assert!(error.to_string().contains("alt is already"), "{error}");
}

#[test]
fn rejects_alt_after_conflicting_quality() {
    for symbol in ["Cmalt", "Cdimalt", "Caugalt", "Csus4alt", "Cmaj7alt"] {
        let error = analyze_symbol(symbol).expect_err("conflicting alt should fail");
        assert!(
            error.to_string().contains("alt is already"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn rejects_descriptors_that_would_drop_tokens() {
    for symbol in ["Cmaj7alt", "Caltmaj7", "C5add9", "C5maj7"] {
        let error = analyze_symbol(symbol).expect_err("invalid descriptor should fail");
        assert!(
            error.to_string().contains("invalid chord descriptor"),
            "{symbol}: {error}"
        );
    }
}

#[test]
fn preserves_multiple_alterations_on_the_same_degree() {
    let (symbol, formula) = analyze_symbol("C7b9#9").unwrap();
    assert_eq!(symbol.name(), "C7b9#9");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "3", "5", "b7", "b9", "#9"]);

    let (symbol, formula) = analyze_symbol("C13#9b9").unwrap();
    assert_eq!(symbol.name(), "C13b9#9");
    let intervals = formula
        .tones
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    assert_eq!(intervals, ["1", "3", "5", "b7", "b9", "#9", "13"]);
}

#[test]
fn generates_voicings_for_altered_thirteenths() {
    let shapes = voicings(
        "C13b9",
        VoicingOptions {
            min_fret: 0,
            max_fret: 12,
            max_span: 4,
            mode: VoicingMode::Curated { limit: 5 },
        },
    )
    .unwrap();
    assert!(!shapes.is_empty());
}

#[test]
fn generates_voicings_for_alt_chords() {
    let shapes = voicings("Calt", VoicingOptions::default()).unwrap();
    assert!(!shapes.is_empty());
    assert!(
        shapes
            .iter()
            .all(|shape| shape.notes.iter().any(|note| note == "Db"))
    );
}

#[test]
fn generates_non_chord_slash_bass_voicings() {
    let shapes = voicings("C/D", VoicingOptions::default()).unwrap();
    assert!(!shapes.is_empty());
    assert!(
        shapes
            .iter()
            .all(|shape| shape.notes.first().is_some_and(|note| note == "D"))
    );
    assert!(shapes.iter().any(|shape| {
        ["C", "E", "G"]
            .iter()
            .all(|note| shape.notes.iter().any(|played| played == note))
    }));
}

#[test]
fn identifies_non_chord_slash_bass_voicings() {
    let result = identify("xx0010").unwrap();
    assert_eq!(
        result.primary.as_ref().expect("primary analysis").symbol,
        "C/D"
    );
    let aliases = result
        .aliases
        .iter()
        .map(|analysis| analysis.symbol.as_str())
        .collect::<Vec<_>>();
    assert!(!aliases.iter().any(|alias| alias.contains("#5no5")));
}

#[test]
fn identifies_non_chord_slash_bass_voicings_with_omissions() {
    let result = identify("xx0x10").unwrap();
    let primary = result.primary.as_ref().expect("primary analysis");
    assert_eq!(primary.symbol, "C/D");
    assert_eq!(primary.omissions, ["5"]);
}

#[test]
fn simple_inverted_dyads_hide_optional_fifth_omission_in_symbol() {
    let result = identify("476xxx").unwrap();
    let primary = result.primary.as_ref().expect("primary analysis");
    assert_eq!(primary.symbol, "E/G#");
    assert_eq!(primary.omissions, ["5"]);

    let result = identify("32x01x").unwrap();
    let primary = result.primary.as_ref().expect("primary analysis");
    assert_eq!(primary.symbol, "Gadd11");
    assert_eq!(primary.omissions, ["5"]);
}

#[test]
fn generated_non_chord_slash_voicings_identify_back_as_primary() {
    for (chord, expected_shape) in [("C/Eb", "x65550"), ("C/Ab", "435553"), ("C/Db", "x45550")] {
        let shapes = voicings(
            chord,
            VoicingOptions {
                mode: VoicingMode::Curated { limit: 1 },
                ..VoicingOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            shapes.first().map(|shape| shape.compact.as_str()),
            Some(expected_shape)
        );

        let result = identify(expected_shape).unwrap();
        assert_eq!(
            result
                .primary
                .as_ref()
                .map(|analysis| analysis.symbol.as_str()),
            Some(chord),
            "{chord}: {result:?}"
        );
        assert_eq!(
            result.notes.first().map(|note| note.note.to_string()),
            chord.split('/').nth(1).map(str::to_owned),
            "{chord}: {result:?}"
        );
    }
}

#[test]
fn inferred_omissions_do_not_create_single_pitch_chords() {
    for fingering in ["x3xxxx", "x3x5xx"] {
        let result = identify(fingering).unwrap();
        assert_eq!(result.primary, None, "{fingering}: {result:?}");
        assert!(result.aliases.is_empty(), "{fingering}: {result:?}");
    }

    let fingering = "xx0x1x";
    let result = identify(fingering).unwrap();
    let symbols = result
        .primary
        .iter()
        .chain(result.aliases.iter())
        .map(|analysis| analysis.symbol.as_str())
        .collect::<Vec<_>>();
    assert!(
        symbols.iter().all(|symbol| !symbol.starts_with("C5no5")),
        "{fingering}: {symbols:?}"
    );

    let shapes = voicings(
        "C",
        VoicingOptions {
            max_fret: 5,
            max_span: 5,
            mode: VoicingMode::All,
            ..VoicingOptions::default()
        },
    )
    .unwrap();
    assert!(shapes.iter().all(|shape| {
        let distinct = shape
            .notes
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        distinct.len() >= 2
    }));
}

#[test]
fn generated_chord_tone_slash_voicings_identify_back_as_slash_chords() {
    let shapes = voicings("C/G", VoicingOptions::default()).unwrap();
    assert!(
        shapes.iter().any(|shape| shape.compact == "332010"),
        "{shapes:?}"
    );

    for fingering in ["332010", "xxx010"] {
        let result = identify(fingering).unwrap();
        assert_eq!(
            result.primary.as_ref().expect("primary analysis").symbol,
            "C/G",
            "{fingering}"
        );
    }
}

#[test]
fn identifies_edge_enharmonic_roots_as_theoretical_aliases() {
    for (compact, expected) in [
        ("x24442", "Cb"),
        ("133211", "E#"),
        ("022100", "Fb"),
        ("x32010", "B#"),
    ] {
        let result = identify(compact).unwrap();
        assert!(
            result.aliases.iter().any(|analysis| {
                analysis.symbol == expected && analysis.class == AnalysisClass::TheoreticalAlias
            }),
            "{compact} should include {expected}: {result:#?}"
        );
    }
}

#[test]
fn ranks_common_c_shapes_above_weird_valid_shapes() {
    let shapes = voicings(
        "C",
        VoicingOptions {
            min_fret: 0,
            max_fret: 12,
            max_span: 4,
            mode: VoicingMode::Curated { limit: 15 },
        },
    )
    .unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert_eq!(compact.first().copied(), Some("x32010"));
    assert!(compact.contains(&"x35553"));
    assert!(!compact.contains(&"x35510"));
    assert!(!compact.contains(&"8-10-10-9-8-0"));
}

#[test]
fn diversifies_minor_voicings_across_positions() {
    let shapes = voicings(
        "Em",
        VoicingOptions {
            min_fret: 0,
            max_fret: 12,
            max_span: 4,
            mode: VoicingMode::Curated { limit: 15 },
        },
    )
    .unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert_eq!(compact.first().copied(), Some("022000"));
    assert!(compact.contains(&"x79987"));
    assert!(compact.contains(&"079987"));
    assert!(!compact.contains(&"0x200x"));
}

#[test]
fn ranks_canonical_open_and_barre_shapes_first() {
    let first_e = voicings("E", VoicingOptions::default())
        .unwrap()
        .first()
        .map(|shape| shape.compact.clone());
    assert_eq!(first_e.as_deref(), Some("022100"));

    let first_g = voicings("G", VoicingOptions::default())
        .unwrap()
        .first()
        .map(|shape| shape.compact.clone());
    assert_eq!(first_g.as_deref(), Some("320003"));

    let first_d = voicings("D", VoicingOptions::default())
        .unwrap()
        .first()
        .map(|shape| shape.compact.clone());
    assert_eq!(first_d.as_deref(), Some("xx0232"));

    let first_f = voicings("F", VoicingOptions::default())
        .unwrap()
        .first()
        .map(|shape| shape.compact.clone());
    assert_eq!(first_f.as_deref(), Some("133211"));
}

#[test]
fn ranks_complete_g_major_grips_above_fragments() {
    let shapes = voicings("G", VoicingOptions::default()).unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert_eq!(compact.first().copied(), Some("320003"));
    assert!(compact.contains(&"320033"));
    assert!(compact.contains(&"355433"));
    assert!(!compact.contains(&"xxx003"));
    assert!(!compact.contains(&"xxx007"));
    assert!(!compact.contains(&"32x43x"));
    assert!(!compact.contains(&"32x433"));

    let open_full = compact.iter().position(|shape| *shape == "320033").unwrap();
    if let Some(open_partial) = compact.iter().position(|shape| *shape == "32000x") {
        assert!(open_full < open_partial);
    }

    let barre_full = compact.iter().position(|shape| *shape == "355433").unwrap();
    if let Some(barre_partial) = compact.iter().position(|shape| *shape == "35543x") {
        assert!(barre_full < barre_partial);
    }
}

#[test]
fn includes_common_omitted_fifth_dominant_voicings() {
    let shapes = voicings("C7", VoicingOptions::default()).unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert_eq!(compact.first().copied(), Some("x32310"));
    assert!(compact.contains(&"x35353"));
    assert!(!compact.contains(&"x32313"));
    let omissions = shapes
        .first()
        .map(|shape| {
            shape
                .omissions
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(omissions, ["5"]);
}

#[test]
fn includes_common_add_sus_slash_and_jazz_grips_in_curated_results() {
    for (chord, expected) in [
        ("Dadd9", "x54230"),
        ("Cadd9", "x32030"),
        ("Asus4", "x02230"),
        ("D/F#", "2x0232"),
        ("G/B", "x20033"),
        ("Dm7", "xx0211"),
        ("G7", "3x343x"),
        ("C7#9", "x3234x"),
    ] {
        let shapes = voicings(
            chord,
            VoicingOptions {
                mode: VoicingMode::Curated { limit: 40 },
                ..VoicingOptions::default()
            },
        )
        .unwrap();
        let compact = shapes
            .iter()
            .map(|shape| shape.compact.as_str())
            .collect::<Vec<_>>();

        assert!(compact.contains(&expected), "{chord}: {compact:?}");
    }
}

#[test]
fn bounded_default_voicings_match_unbounded_ranking() {
    for chord in ["C", "G", "Em", "C7", "G7", "C/D", "C13b9", "Calt"] {
        let options = VoicingOptions::default();
        let bounded = voicings(chord, options).unwrap();
        let all = voicings(
            chord,
            VoicingOptions {
                mode: VoicingMode::All,
                ..options
            },
        )
        .unwrap();
        let expected =
            rank_diverse_voicing_candidates(candidates_from_voicings(&all), DEFAULT_LIMIT);

        assert_eq!(candidates_from_voicings(&bounded), expected, "{chord}");
    }

    let dadgad = GuitarTuning::parse("DADGAD").unwrap();
    let options = VoicingOptions::default();
    let bounded = voicings_with_tuning("Dsus4", dadgad, options).unwrap();
    let all = voicings_with_tuning(
        "Dsus4",
        dadgad,
        VoicingOptions {
            mode: VoicingMode::All,
            ..options
        },
    )
    .unwrap();
    let expected = rank_diverse_voicing_candidates(candidates_from_voicings(&all), DEFAULT_LIMIT);
    assert_eq!(candidates_from_voicings(&bounded), expected);
}

#[test]
fn voicing_score_breakdown_matches_generated_score() {
    let tuning = STANDARD_TUNING;
    let score_context = VoicingScoreContext::new("C13b9", tuning).unwrap();
    let shapes = voicings(
        "C13b9",
        VoicingOptions {
            mode: VoicingMode::Curated { limit: 8 },
            ..VoicingOptions::default()
        },
    )
    .unwrap();

    for shape in shapes {
        let breakdown = score_context.breakdown(&shape.frets).unwrap();
        assert_eq!(breakdown.total, shape.score, "{}", shape.compact);
    }
}

#[test]
fn voicing_score_breakdown_rejects_extra_pitches() {
    let score_context = VoicingScoreContext::new("C", STANDARD_TUNING).unwrap();
    let error = score_context
        .breakdown(&[None, Some(3), Some(0), Some(0), Some(1), Some(0)])
        .expect_err("non-chord pitch should fail");

    assert!(error.to_string().contains("outside the chord"), "{error}");
}

#[test]
fn zero_limit_skips_default_voicing_collection() {
    let shapes = voicings(
        "C",
        VoicingOptions {
            mode: VoicingMode::Curated { limit: 0 },
            ..VoicingOptions::default()
        },
    )
    .unwrap();
    assert!(shapes.is_empty());
}

#[test]
fn curated_limit_has_a_hard_cap() {
    let error = voicings(
        "C",
        VoicingOptions {
            mode: VoicingMode::Curated {
                limit: MAX_LIMIT + 1,
            },
            ..VoicingOptions::default()
        },
    )
    .expect_err("oversized curated limit should fail");

    assert!(error.to_string().contains("invalid limit"), "{error}");
}

#[test]
fn generates_rootless_voicings_when_root_is_omitted() {
    let shapes = voicings("Cno1", VoicingOptions::default()).unwrap();
    assert!(!shapes.is_empty());
    assert!(
        shapes
            .iter()
            .all(|shape| shape.notes.iter().all(|note| note != "C"))
    );
}

#[test]
fn automatically_generates_rootless_voicings_for_extended_chords() {
    let shapes = voicings(
        "Cmaj7",
        VoicingOptions {
            mode: VoicingMode::All,
            ..VoicingOptions::default()
        },
    )
    .unwrap();

    assert!(shapes.iter().any(|shape| {
        shape.omissions.iter().map(String::as_str).eq(["1"])
            && shape.notes.iter().all(|note| note != "C")
    }));
    assert!(shapes.iter().any(|shape| {
        shape.omissions.iter().map(String::as_str).eq(["3"])
            && shape.notes.iter().all(|note| note != "E")
    }));
}

#[test]
fn keeps_compact_jazz_shells_in_the_curated_results() {
    let shapes = voicings("G7", VoicingOptions::default()).unwrap();
    let compact = shapes
        .iter()
        .map(|shape| shape.compact.as_str())
        .collect::<Vec<_>>();

    assert!(compact.contains(&"3x343x"));
}

#[test]
fn min_fret_generates_only_shapes_inside_the_requested_range() {
    let shapes = voicings(
        "C",
        VoicingOptions {
            min_fret: 12,
            max_fret: 15,
            max_span: 4,
            mode: VoicingMode::Curated { limit: 15 },
        },
    )
    .unwrap();

    assert!(!shapes.is_empty());
    for shape in shapes {
        let fretted = shape.frets.iter().flatten().copied().collect::<Vec<_>>();
        assert!(!fretted.is_empty(), "{}", shape.compact);
        assert!(
            fretted.iter().all(|fret| (12..=15).contains(fret)),
            "{}: {:?}",
            shape.compact,
            fretted
        );
    }
}

#[test]
fn infers_rootless_and_no_third_omitted_analyses() {
    let rootless = identify("xx2000").unwrap();
    let aliases = rootless
        .aliases
        .iter()
        .map(|analysis| analysis.symbol.as_str())
        .collect::<Vec<_>>();
    assert!(aliases.contains(&"Cmaj7no1/E"));

    assert_eq!(primary("x353xx"), "C7no3");
    assert_eq!(primary("x354xx"), "Cmaj7no3");
}

#[test]
fn classifies_theoretical_aliases_away_from_default_display() {
    let result = identify("x12010").unwrap();
    let edge_spelling_alias = result
        .aliases
        .iter()
        .find(|analysis| analysis.symbol == "B#7/A#")
        .expect("theoretical edge-spelling alias");
    assert_eq!(edge_spelling_alias.class, AnalysisClass::TheoreticalAlias);

    let useful_alias = result
        .aliases
        .iter()
        .find(|analysis| analysis.symbol == "C/Bb")
        .expect("useful slash alias");
    assert_eq!(useful_alias.class, AnalysisClass::UsefulAlias);
}

#[test]
fn all_voicings_returns_more_than_the_default_limit() {
    let limited = voicings("C", VoicingOptions::default()).unwrap();
    let all = voicings(
        "C",
        VoicingOptions {
            mode: VoicingMode::All,
            ..VoicingOptions::default()
        },
    )
    .unwrap();

    assert_eq!(limited.len(), DEFAULT_LIMIT);
    assert!(all.len() > limited.len());
    assert_eq!(
        all.first().map(|shape| shape.compact.as_str()),
        Some("x32010")
    );
}

#[test]
fn wide_curated_pruning_matches_unbounded_ranking() {
    let options = VoicingOptions {
        max_fret: 12,
        max_span: 8,
        mode: VoicingMode::Curated { limit: 50 },
        ..VoicingOptions::default()
    };
    let bounded = voicings("Calt", options).unwrap();
    let all = voicings(
        "Calt",
        VoicingOptions {
            mode: VoicingMode::All,
            ..options
        },
    )
    .unwrap();
    let expected = rank_diverse_voicing_candidates(candidates_from_voicings(&all), 50);

    assert_eq!(candidates_from_voicings(&bounded), expected);
}

#[test]
fn all_voicings_has_a_hard_result_cap() {
    let error = voicings(
        "Calt",
        VoicingOptions {
            max_fret: 30,
            max_span: 30,
            mode: VoicingMode::All,
            ..VoicingOptions::default()
        },
    )
    .expect_err("pathological --all result should fail");

    assert!(error.to_string().contains("--all cap"), "{error}");
}

#[test]
fn rejects_frets_outside_standard_guitar_range() {
    let error = identify("31-x-x-x-x-x").expect_err("fret above 30 should fail");
    assert!(
        error.to_string().contains("standard guitar range"),
        "{error}"
    );

    let error = voicings(
        "C",
        VoicingOptions {
            min_fret: 31,
            ..VoicingOptions::default()
        },
    )
    .expect_err("min fret above 30 should fail");
    assert!(
        error.to_string().contains("standard guitar range"),
        "{error}"
    );

    let error = voicings(
        "C",
        VoicingOptions {
            max_fret: 31,
            ..VoicingOptions::default()
        },
    )
    .expect_err("max fret above 30 should fail");
    assert!(
        error.to_string().contains("standard guitar range"),
        "{error}"
    );

    let error = voicings(
        "C",
        VoicingOptions {
            max_span: 31,
            ..VoicingOptions::default()
        },
    )
    .expect_err("max span above 30 should fail");
    assert!(
        error.to_string().contains("standard guitar range"),
        "{error}"
    );

    let error = voicings(
        "C",
        VoicingOptions {
            min_fret: 13,
            max_fret: 12,
            ..VoicingOptions::default()
        },
    )
    .expect_err("min fret above max fret should fail");
    assert!(error.to_string().contains("min_fret"), "{error}");
}

#[test]
fn chord_formulas_are_interval_invariant_across_roots() {
    for root in [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ] {
        for suffix in ["", "m", "7", "maj7", "m7b5", "dim7", "alt"] {
            let (_, formula) = analyze_symbol(&format!("{root}{suffix}")).unwrap();
            let intervals = formula
                .tones
                .iter()
                .map(|tone| tone.interval.to_string())
                .collect::<Vec<_>>();
            let expected = match suffix {
                "" => vec!["1", "3", "5"],
                "m" => vec!["1", "b3", "5"],
                "7" => vec!["1", "3", "5", "b7"],
                "maj7" => vec!["1", "3", "5", "7"],
                "m7b5" => vec!["1", "b3", "b5", "b7"],
                "dim7" => vec!["1", "b3", "b5", "bb7"],
                "alt" => vec!["1", "3", "b7", "b9", "#9", "b13"],
                _ => unreachable!(),
            };
            assert_eq!(intervals, expected, "{root}{suffix}");
        }
    }
}
