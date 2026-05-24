# Chordsmith

Chordsmith is a local Rust chord engine for guitar, extended-range guitar, and
ukulele. It identifies chords from fingerings and generates voicings from
interval math instead of a chord-shape database.

```sh
chordsmith --version
chordsmith identify 022000
chordsmith identify --instrument guitar7 0000000
chordsmith identify --instrument ukulele 2010
chordsmith voicings Em
```

## Status

Version 1.3.0. The engine separates chord theory from CLI formatting, uses
deterministic ranking, and treats chord names as analyses over pitch-class sets
rather than as chart entries.

Distribution is through local builds and GitHub release artifacts; workspace
crates are not published to crates.io.

## Quick Start

```sh
cargo build --release -p chordsmith-cli
./target/release/chordsmith identify 022000
./target/release/chordsmith voicings C6/9
```

Install on your PATH:

```sh
cargo install --path crates/chordsmith-cli
chordsmith --help
```

## CLI

```sh
# Identify a low-to-high guitar fingering.
chordsmith identify 022000
chordsmith identify 10-x-11-9-8-x
chordsmith identify --tuning DADGAD 000000
chordsmith identify --instrument guitar7 0000000
chordsmith identify --tuning F#BEADGBE 00000000

# Identify standard high-G ukulele string-order fingerings.
chordsmith identify --instrument ukulele 2010
chordsmith identify --tuning GCEA 0003
chordsmith identify --instrument ukulele --tuning G3,C4,E4,A4 0000

# Emit structured output.
chordsmith identify --json x12010

# Generate guitar voicings from a chord symbol.
chordsmith voicings Em
chordsmith voicings --tuning D,A,D,G,A,D Dsus4
chordsmith voicings --max-fret 15 --limit 25 C13b9
chordsmith voicings --min-fret 12 C
chordsmith voicings --min-fret 12 --max-fret 15 C
chordsmith voicings --all C
chordsmith voicings Calt

# Generate ukulele voicings.
chordsmith voicings --instrument ukulele C
chordsmith voicings --tuning GCEA Am

# Show parsed interval content for a chord symbol.
chordsmith analyze C7#9#11
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
cargo llvm-cov -p chordsmith-core --lib --locked --summary-only --fail-under-lines 90
cargo bench -p chordsmith-core --bench perf --locked
cargo deny check
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license
