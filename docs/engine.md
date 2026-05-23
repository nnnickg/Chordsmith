# Engine

## Invariants

- Guitar fingerings are read low string to high string.
- `x` means muted.
- Compact six-character input supports frets `0..9`; dashed input supports
  multi-digit frets, for example `10-x-11-9-8-x`.
- Analysis is pitch-class based, but output spelling is chord-aware. A `C7`
  voicing with `Bb` in the bass should render as `C7/Bb`, not `C7/A#`.
- Chord names are ranked analyses, not a single mathematical truth. The same
  pitch-class set can have multiple valid names.
- Standard guitar fret and non-open span range is `0..=24`.
- `alt` is a concrete altered dominant shorthand: `1 3 b7 b9 #9 b13`.
- Default text output hides theoretical aliases; JSON keeps alias class data.

## No Shape Database

Voicings are generated from:

1. tuning pitch classes;
2. parsed chord-symbol intervals;
3. maximum fret;
4. maximum non-open fret span;
5. bass constraint for slash chords.

There is no built-in `C = x32010` style table.

Generated voicings may omit the natural perfect fifth for four-or-more-tone
chords. Those omissions are included in structured output and printed in the
text table when present.
