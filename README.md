# ChordClaw

ChordClaw is a local Rust chord engine for guitar, extended-range guitar, and
ukulele. It identifies chords from fingerings and generates voicings from
interval math instead of a chord-shape database.

```sh
chordclaw --version
chordclaw identify 022000
chordclaw identify --instrument guitar7 0000000
chordclaw identify --instrument ukulele 2010
chordclaw voicings Em
```

## Status

Version 1.3.5. The engine separates chord theory from CLI formatting, uses
deterministic ranking, and treats chord names as analyses over pitch-class sets
rather than as chart entries.

Distribution is through local builds and GitHub release artifacts; workspace
crates are not published to crates.io.

## Quick Start

```sh
cargo build --release -p chordclaw-cli
./target/release/chordclaw identify 022000
./target/release/chordclaw voicings C6/9
```

Install on your PATH:

```sh
cargo install --path crates/chordclaw-cli
chordclaw --help
```

## CLI

```sh
# Identify a low-to-high guitar fingering.
chordclaw identify 022000
chordclaw identify 10-x-11-9-8-x
chordclaw identify --tuning DADGAD 000000
chordclaw identify --instrument guitar7 0000000
chordclaw identify --tuning F#BEADGBE 00000000

# Identify standard high-G ukulele string-order fingerings.
chordclaw identify --instrument ukulele 2010
chordclaw identify --tuning GCEA 0003
chordclaw identify --instrument ukulele --tuning G3,C4,E4,A4 0000

# Emit structured output.
chordclaw identify --json x12010

# Generate guitar voicings from a chord symbol.
chordclaw voicings Em
chordclaw voicings --tuning D,A,D,G,A,D Dsus4
chordclaw voicings --max-fret 15 --limit 25 C13b9
chordclaw voicings --min-fret 12 C
chordclaw voicings --min-fret 12 --max-fret 15 C
chordclaw voicings --all C
chordclaw voicings Calt

# Generate ukulele voicings.
chordclaw voicings --instrument ukulele C
chordclaw voicings --tuning GCEA Am

# Show parsed interval content for a chord symbol.
chordclaw analyze C7#9#11
```

`--instrument` accepts `guitar`, `guitar7`, `guitar8`, or `ukulele`; six-string
guitar remains the default. `--tuning` accepts four-, six-, seven-, or
eight-string note names either compact (`GCEA`, `DADGAD`, `BEADGBE`,
`F#BEADGBE`, `EbAbDbGbBbEb`) or separated (`G,C,E,A`, `D,A,D,G,A,D`). Tuning
notes may include octaves (`G3,C4,E4,A4`, `E2,A2,D3,G3,B3,E4`); when any
octave is present, every tuning note must have one. Standard ukulele uses
high-G re-entrant `GCEA`, and direct transpositions of that four-string interval
pattern infer the same re-entrant contour, except baritone `DGBE` defaults to a
linear low-D contour. Use explicit octaves for low-G ukulele, high-D baritone,
or custom re-entrant tunings.
`--min-fret`, `--max-fret`, and `--max-span` are intentionally capped at 30.
`--min-fret` values above 0 exclude open strings. When `--min-fret` is 12 or
higher and `--max-fret` is omitted, the CLI scans through fret 30. `alt` means
the concrete altered dominant set `1 3 b7 b9 #9 b13`. `--all` is capped at
25,000 generated voicings; `--limit` is capped at 1,000 curated voicings.
Narrow broad searches with fret and span flags.

Identification uses a static generated grammar of common tertian, suspended,
added-tone, altered-dominant, diminished, half-diminished, slash, and inferred
omission analyses. It is intentionally not an unbounded symbolic theorem prover
for arbitrary exotic chord naming systems.

Engine invariants are documented in [docs/engine.md](docs/engine.md).

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo llvm-cov -p chordclaw-core --lib --locked --summary-only --fail-under-lines 90
cargo bench -p chordclaw-core --bench perf --locked
cargo deny check
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license
