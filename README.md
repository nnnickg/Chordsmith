# Chordsmith

Chordsmith is a local Rust chord engine for guitar. It identifies chords from
fingerings and generates voicings from interval math instead of a chord-shape
database.

```sh
chordsmith --version
chordsmith identify 022000
chordsmith voicings Em
```

## Status

Early implementation. The engine already separates chord theory from CLI
formatting, uses deterministic ranking, and treats chord names as analyses over
pitch-class sets rather than as chart entries.

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

# Emit structured output.
chordsmith identify --json x12010

# Generate guitar voicings from a chord symbol.
chordsmith voicings Em
chordsmith voicings --max-fret 15 --limit 25 C13b9
chordsmith voicings --all C
chordsmith voicings Calt

# Show parsed interval content for a chord symbol.
chordsmith analyze C7#9#11
```

`--max-fret` and `--max-span` are intentionally capped at 24 for standard
guitar. `alt` means the concrete altered dominant set `1 3 b7 b9 #9 b13`.

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license
