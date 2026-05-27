#![allow(clippy::expect_used, clippy::panic)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use chordclaw_core::{
    GuitarTuning, Instrument, VoicingMode, VoicingOptions, analyze_symbol, identify, voicings,
    voicings_with_tuning,
};

fn main() {
    bench_cold("identify_cold_engine", Duration::from_millis(500), || {
        black_box(identify(black_box("332010")).expect("cold identify 332010"));
    });

    bench("analyze_symbol", 20_000, Duration::from_secs(1), || {
        black_box(analyze_symbol(black_box("C13b9#9")).expect("analyze C13b9#9"));
    });

    bench("identify", 5_000, Duration::from_secs(2), || {
        black_box(identify(black_box("332010")).expect("identify 332010"));
    });

    bench("common_voicings", 200, Duration::from_secs(4), || {
        black_box(voicings(black_box("Cmaj7"), VoicingOptions::default()).expect("voicings Cmaj7"));
    });

    bench("wide_curated_voicings", 1, Duration::from_secs(5), || {
        black_box(
            voicings(
                black_box("Calt"),
                VoicingOptions {
                    min_fret: 0,
                    max_fret: 24,
                    max_span: 24,
                    mode: VoicingMode::Curated { limit: 1000 },
                },
            )
            .expect("wide curated Calt"),
        );
    });

    bench("worst_allowed_all", 1, Duration::from_secs(5), || {
        black_box(
            voicings(
                black_box("C7"),
                VoicingOptions {
                    min_fret: 0,
                    max_fret: 18,
                    max_span: 6,
                    mode: VoicingMode::All,
                },
            )
            .expect("bounded --all C7"),
        );
    });

    let dadgad = GuitarTuning::parse("DADGAD").expect("DADGAD tuning");
    bench(
        "alternate_tuning_voicings",
        200,
        Duration::from_secs(4),
        || {
            black_box(
                voicings_with_tuning(black_box("Dsus4"), dadgad, VoicingOptions::default())
                    .expect("DADGAD Dsus4 voicings"),
            );
        },
    );

    let ukulele = Instrument::Ukulele.default_tuning();
    bench("ukulele_voicings", 500, Duration::from_secs(2), || {
        black_box(
            voicings_with_tuning(black_box("C"), ukulele, VoicingOptions::default())
                .expect("ukulele C voicings"),
        );
    });
}

fn bench(name: &str, iterations: u32, max_total: Duration, mut run: impl FnMut()) {
    for _ in 0..10 {
        run();
    }

    let started = Instant::now();
    for _ in 0..iterations {
        run();
    }
    let elapsed = started.elapsed();
    let per_op = elapsed.as_secs_f64() / f64::from(iterations);
    println!("{name}: {elapsed:?} total, {per_op:.9}s/op over {iterations} iterations");
    assert!(
        elapsed <= max_total,
        "{name} exceeded benchmark contract: {elapsed:?} > {max_total:?}"
    );
}

fn bench_cold(name: &str, max_total: Duration, mut run: impl FnMut()) {
    let started = Instant::now();
    run();
    let elapsed = started.elapsed();
    println!("{name}: {elapsed:?} first call");
    assert!(
        elapsed <= max_total,
        "{name} exceeded benchmark contract: {elapsed:?} > {max_total:?}"
    );
}
