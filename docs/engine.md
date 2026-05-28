# Engine

## Invariants

- Guitar and ukulele fingerings are read in string order.
- The engine supports four-string ukulele plus six-, seven-, and eight-string
  guitar. Standard EADGBE guitar is the default; standard ukulele is high-G
  re-entrant GCEA.
- CLI identification infers the default instrument from fingering width when no
  explicit instrument or tuning is supplied: 4, 6, 7, or 8 strings map to
  ukulele, guitar, guitar7, or guitar8.
- Tuning may be note-only (`DADGAD`, `GCEA`) or octave-explicit
  (`G3,C4,E4,A4`). If any tuning note has an octave, all tuning notes must.
- `x` means muted.
- Compact input supports one digit per string; dashed input supports multi-digit
  frets, for example `10-x-11-9-8-x`.
- Analysis is pitch-class based, but output spelling is chord-aware. A `C7`
  voicing with `Bb` in the bass should render as `C7/Bb`, not `C7/A#`.
- Slash bass is modeled as a required lowest pitch. If the slash note is not a
  chord tone, voicing generation searches the chord tones plus that bass pitch.
- Chord names are ranked analyses, not a single mathematical truth. The same
  pitch-class set can have multiple valid names.
- Identification candidate records and pitch-set indexes are generated at build
  time from the theory rules. Runtime lookup does not build a process-local
  candidate cache.
- Parsed note names allow natural, single, and double accidentals. Opposite
  accidentals stop root parsing so `C#b5` means `C#` plus `b5`, while bare
  invalid spellings such as `C#b` are rejected.
- Fret and non-open span range is `0..=30`.
- `alt` is a concrete altered dominant shorthand: `1 3 b7 b9 #9 b13`.
- Default text output hides theoretical aliases; JSON keeps alias class data.

## No Shape Database

Voicings are generated from:

1. tuning pitches;
2. parsed chord-symbol intervals;
3. maximum fret;
4. maximum non-open fret span;
5. required bass constraint for slash chords, preferred root bass otherwise.

There is no built-in `C = x32010` style table.

Generated voicings use the same omitted-tone rules as identification: the root
may be omitted from four-or-more-tone chords, the third from seventh-or-richer
chords, and the fifth when musically redundant. Those omissions are included in
structured output and printed in the text table when present.

## Voicing Ranking

Ranking is deterministic and computed from the generated fingering, not from a
shape database. The scorer favors complete low-position and closed grips, root
bass for non-slash chords, compact fret spans, playable barre explanations, and
useful shell voicings for upper-structure chords.

Post-bonus penalties cannot be erased by open-position bonuses. They demote
triad omissions, excessive internal or trailing mutes, sparse duplicate stacks,
open-position shapes that fret past an already-valid open chord tone, and grips
that require more than four fingers after barre candidates are accounted for.

The default list is curated for quality first and diversity second. Diversity
can only choose among nearby raw scores; `--all` returns every generated voicing
sorted by raw score.

`chordclaw voicings --explain` exposes the selected voicings' score components;
the default generation path does not retain per-candidate diagnostics.

Default voicing collection is bounded but exact: the engine keeps the raw top-k
score frontier plus the maximum diversity window, then runs the same diversity
ranker on that reduced pool. This preserves the same result as ranking every
generated voicing while avoiding an unbounded retained candidate list for normal
output.
