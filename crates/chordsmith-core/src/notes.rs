use std::cmp::Ordering;
use std::fmt::{self, Write as _};

use serde::{Serialize, Serializer, ser::SerializeStruct};

use crate::parse::{normalize_chart_glyphs, parse_note_prefix};
use crate::{
    ChordsmithError, GUITAR_STRING_COUNT, GUITAR7_STRING_COUNT, GUITAR8_STRING_COUNT,
    MAX_NOTE_ACCIDENTALS, MAX_STANDARD_FRET, MAX_STRING_COUNT, UKULELE_STRING_COUNT,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PitchClass(pub(crate) u8);

impl PitchClass {
    pub const fn new(value: i16) -> Self {
        let mut normalized = value % 12;
        if normalized < 0 {
            normalized += 12;
        }
        Self(normalized as u8)
    }

    pub const fn value(self) -> u8 {
        self.0
    }

    pub const fn transpose(self, semitones: i16) -> Self {
        Self::new(self.0 as i16 + semitones)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub enum NoteLetter {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl NoteLetter {
    pub(crate) fn from_ascii(ch: char) -> Option<Self> {
        match ch.to_ascii_uppercase() {
            'C' => Some(Self::C),
            'D' => Some(Self::D),
            'E' => Some(Self::E),
            'F' => Some(Self::F),
            'G' => Some(Self::G),
            'A' => Some(Self::A),
            'B' => Some(Self::B),
            _ => None,
        }
    }

    pub(crate) const fn base_pitch(self) -> i16 {
        match self {
            Self::C => 0,
            Self::D => 2,
            Self::E => 4,
            Self::F => 5,
            Self::G => 7,
            Self::A => 9,
            Self::B => 11,
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::C => 0,
            Self::D => 1,
            Self::E => 2,
            Self::F => 3,
            Self::G => 4,
            Self::A => 5,
            Self::B => 6,
        }
    }

    const fn char(self) -> char {
        match self {
            Self::C => 'C',
            Self::D => 'D',
            Self::E => 'E',
            Self::F => 'F',
            Self::G => 'G',
            Self::A => 'A',
            Self::B => 'B',
        }
    }

    pub(crate) fn advance(self, steps: u8) -> Self {
        const LETTERS: [NoteLetter; 7] = [
            NoteLetter::C,
            NoteLetter::D,
            NoteLetter::E,
            NoteLetter::F,
            NoteLetter::G,
            NoteLetter::A,
            NoteLetter::B,
        ];
        let idx = (self.index() + usize::from(steps)) % LETTERS.len();
        LETTERS[idx]
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct NoteName {
    pub(crate) letter: NoteLetter,
    pub(crate) accidental: i8,
}

impl Default for NoteName {
    fn default() -> Self {
        Self {
            letter: NoteLetter::C,
            accidental: 0,
        }
    }
}

impl NoteName {
    pub(crate) const fn const_new(letter: NoteLetter, accidental: i8) -> Self {
        assert!(accidental >= -2 && accidental <= 2);
        Self { letter, accidental }
    }

    pub fn new(letter: NoteLetter, accidental: i8) -> Result<Self, ChordsmithError> {
        if accidental.unsigned_abs() > MAX_NOTE_ACCIDENTALS {
            return Err(ChordsmithError::new(format!(
                "too many accidentals for note '{}{}'",
                letter.char(),
                accidental_text(accidental)
            )));
        }
        Ok(Self { letter, accidental })
    }

    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        let normalized = normalize_chart_glyphs(input);
        let (note, rest) = parse_note_prefix(normalized.as_ref())?;
        if rest.is_empty() {
            Ok(note)
        } else {
            Err(ChordsmithError::new(format!("invalid note '{input}'")))
        }
    }

    pub const fn letter(self) -> NoteLetter {
        self.letter
    }

    pub const fn accidental(self) -> i8 {
        self.accidental
    }

    pub const fn pitch_class(self) -> PitchClass {
        PitchClass::new(self.letter.base_pitch() + self.accidental as i16)
    }

    pub(crate) fn spell_for_pitch(letter: NoteLetter, pitch: PitchClass) -> Self {
        let target = i16::from(pitch.value());
        let base = letter.base_pitch();
        let mut accidental = target - base;
        while accidental > 6 {
            accidental -= 12;
        }
        while accidental < -6 {
            accidental += 12;
        }
        Self {
            letter,
            accidental: accidental as i8,
        }
    }

    pub fn simple_for_pitch(pitch: PitchClass, prefer_flats: bool) -> Self {
        let names = if prefer_flats {
            SIMPLE_FLAT_NAMES
        } else {
            SIMPLE_SHARP_NAMES
        };
        names[usize::from(pitch.value())]
    }
}

impl fmt::Display for NoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char(self.letter.char())?;
        write_accidentals(f, self.accidental)
    }
}

impl Serialize for NoteName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

fn write_accidentals(f: &mut fmt::Formatter<'_>, accidental: i8) -> fmt::Result {
    let ch = match accidental.cmp(&0) {
        Ordering::Less => 'b',
        Ordering::Equal => return Ok(()),
        Ordering::Greater => '#',
    };
    for _ in 0..accidental.unsigned_abs() {
        f.write_char(ch)?;
    }
    Ok(())
}

fn accidental_text(accidental: i8) -> String {
    match accidental.cmp(&0) {
        Ordering::Less => "b".repeat(accidental.unsigned_abs() as usize),
        Ordering::Equal => String::new(),
        Ordering::Greater => "#".repeat(accidental.unsigned_abs() as usize),
    }
}

const SIMPLE_SHARP_NAMES: [NoteName; 12] = [
    NoteName::const_new(NoteLetter::C, 0),
    NoteName::const_new(NoteLetter::C, 1),
    NoteName::const_new(NoteLetter::D, 0),
    NoteName::const_new(NoteLetter::D, 1),
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::F, 0),
    NoteName::const_new(NoteLetter::F, 1),
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::G, 1),
    NoteName::const_new(NoteLetter::A, 0),
    NoteName::const_new(NoteLetter::A, 1),
    NoteName::const_new(NoteLetter::B, 0),
];

const SIMPLE_FLAT_NAMES: [NoteName; 12] = [
    NoteName::const_new(NoteLetter::C, 0),
    NoteName::const_new(NoteLetter::D, -1),
    NoteName::const_new(NoteLetter::D, 0),
    NoteName::const_new(NoteLetter::E, -1),
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::F, 0),
    NoteName::const_new(NoteLetter::G, -1),
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::A, -1),
    NoteName::const_new(NoteLetter::A, 0),
    NoteName::const_new(NoteLetter::B, -1),
    NoteName::const_new(NoteLetter::B, 0),
];

pub const STANDARD_TUNING_NOTES: [NoteName; GUITAR_STRING_COUNT] = [
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::A, 0),
    NoteName::const_new(NoteLetter::D, 0),
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::B, 0),
    NoteName::const_new(NoteLetter::E, 0),
];

pub const STANDARD_7_STRING_TUNING_NOTES: [NoteName; GUITAR7_STRING_COUNT] = [
    NoteName::const_new(NoteLetter::B, 0),
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::A, 0),
    NoteName::const_new(NoteLetter::D, 0),
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::B, 0),
    NoteName::const_new(NoteLetter::E, 0),
];

pub const STANDARD_8_STRING_TUNING_NOTES: [NoteName; GUITAR8_STRING_COUNT] = [
    NoteName::const_new(NoteLetter::F, 1),
    NoteName::const_new(NoteLetter::B, 0),
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::A, 0),
    NoteName::const_new(NoteLetter::D, 0),
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::B, 0),
    NoteName::const_new(NoteLetter::E, 0),
];

pub const STANDARD_UKULELE_TUNING_NOTES: [NoteName; UKULELE_STRING_COUNT] = [
    NoteName::const_new(NoteLetter::G, 0),
    NoteName::const_new(NoteLetter::C, 0),
    NoteName::const_new(NoteLetter::E, 0),
    NoteName::const_new(NoteLetter::A, 0),
];

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Instrument {
    Guitar,
    Guitar7,
    Guitar8,
    Ukulele,
}

impl Instrument {
    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        match input.trim().to_ascii_lowercase().as_str() {
            "guitar" | "g" => Ok(Self::Guitar),
            "guitar7"
            | "g7"
            | "7"
            | "7string"
            | "7-string"
            | "7-string-guitar"
            | "seven-string"
            | "seven-string-guitar" => Ok(Self::Guitar7),
            "guitar8"
            | "g8"
            | "8"
            | "8string"
            | "8-string"
            | "8-string-guitar"
            | "eight-string"
            | "eight-string-guitar" => Ok(Self::Guitar8),
            "ukulele" | "uke" | "u" => Ok(Self::Ukulele),
            other => Err(ChordsmithError::new(format!(
                "unsupported instrument '{other}': expected guitar, guitar7, guitar8, or ukulele"
            ))),
        }
    }

    pub const fn string_count(self) -> usize {
        match self {
            Self::Guitar => GUITAR_STRING_COUNT,
            Self::Guitar7 => GUITAR7_STRING_COUNT,
            Self::Guitar8 => GUITAR8_STRING_COUNT,
            Self::Ukulele => UKULELE_STRING_COUNT,
        }
    }

    pub const fn default_tuning(self) -> Tuning {
        match self {
            Self::Guitar => STANDARD_TUNING,
            Self::Guitar7 => STANDARD_7_STRING_TUNING,
            Self::Guitar8 => STANDARD_8_STRING_TUNING,
            Self::Ukulele => STANDARD_UKULELE_TUNING,
        }
    }
}

impl fmt::Display for Instrument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Guitar => "guitar",
            Self::Guitar7 => "guitar7",
            Self::Guitar8 => "guitar8",
            Self::Ukulele => "ukulele",
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Tuning {
    instrument: Instrument,
    notes: [NoteName; MAX_STRING_COUNT],
    open_pitches: [i16; MAX_STRING_COUNT],
    string_count: usize,
}

pub type GuitarTuning = Tuning;
pub type UkuleleTuning = Tuning;

impl Tuning {
    pub const fn new(notes: [NoteName; GUITAR_STRING_COUNT]) -> Self {
        let mut out = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
        let mut idx = 0;
        while idx < GUITAR_STRING_COUNT {
            out[idx] = notes[idx];
            idx += 1;
        }
        Self {
            instrument: Instrument::Guitar,
            notes: out,
            open_pitches: ascending_open_pitches(out, GUITAR_STRING_COUNT),
            string_count: GUITAR_STRING_COUNT,
        }
    }

    pub const fn new_ukulele(notes: [NoteName; UKULELE_STRING_COUNT]) -> Self {
        let mut out = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
        let mut idx = 0;
        while idx < UKULELE_STRING_COUNT {
            out[idx] = notes[idx];
            idx += 1;
        }
        Self {
            instrument: Instrument::Ukulele,
            notes: out,
            open_pitches: open_pitches_for(out, UKULELE_STRING_COUNT, Instrument::Ukulele),
            string_count: UKULELE_STRING_COUNT,
        }
    }

    pub const fn new_guitar7(notes: [NoteName; GUITAR7_STRING_COUNT]) -> Self {
        let mut out = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
        let mut idx = 0;
        while idx < GUITAR7_STRING_COUNT {
            out[idx] = notes[idx];
            idx += 1;
        }
        Self {
            instrument: Instrument::Guitar7,
            notes: out,
            open_pitches: ascending_open_pitches(out, GUITAR7_STRING_COUNT),
            string_count: GUITAR7_STRING_COUNT,
        }
    }

    pub const fn new_guitar8(notes: [NoteName; GUITAR8_STRING_COUNT]) -> Self {
        let mut out = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
        let mut idx = 0;
        while idx < GUITAR8_STRING_COUNT {
            out[idx] = notes[idx];
            idx += 1;
        }
        Self {
            instrument: Instrument::Guitar8,
            notes: out,
            open_pitches: ascending_open_pitches(out, GUITAR8_STRING_COUNT),
            string_count: GUITAR8_STRING_COUNT,
        }
    }

    fn from_parsed(
        parsed: ParsedTuning,
        instrument: Option<Instrument>,
    ) -> Result<Self, ChordsmithError> {
        let string_count = parsed.string_count;
        if !is_supported_string_count(string_count) {
            return Err(ChordsmithError::new(format!(
                "expected 4, 6, 7, or 8 tuning notes, got {string_count}"
            )));
        }
        let instrument = instrument.unwrap_or(match string_count {
            UKULELE_STRING_COUNT => Instrument::Ukulele,
            GUITAR7_STRING_COUNT => Instrument::Guitar7,
            GUITAR8_STRING_COUNT => Instrument::Guitar8,
            _ => Instrument::Guitar,
        });
        if instrument.string_count() != string_count {
            return Err(ChordsmithError::new(format!(
                "expected {} tuning notes for {instrument}, got {string_count}",
                instrument.string_count()
            )));
        }
        Ok(Self {
            instrument,
            notes: parsed.notes,
            open_pitches: parsed
                .open_pitches
                .unwrap_or_else(|| open_pitches_for(parsed.notes, string_count, instrument)),
            string_count,
        })
    }

    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        let normalized = normalize_chart_glyphs(input);
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("standard") {
            return Ok(STANDARD_TUNING);
        }

        let parsed = if has_tuning_separators(trimmed) {
            parse_separated_tuning(trimmed, None)?
        } else {
            parse_compact_tuning(trimmed, None)?
        };
        Self::from_parsed(parsed, None)
    }

    pub fn parse_for_instrument(
        input: &str,
        instrument: Instrument,
    ) -> Result<Self, ChordsmithError> {
        let normalized = normalize_chart_glyphs(input);
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("standard") {
            return Ok(instrument.default_tuning());
        }

        let expected = instrument.string_count();
        let parsed = if has_tuning_separators(trimmed) {
            parse_separated_tuning(trimmed, Some(expected))?
        } else {
            parse_compact_tuning(trimmed, Some(expected))?
        };
        debug_assert_eq!(parsed.string_count, expected);
        Self::from_parsed(parsed, Some(instrument))
    }

    pub const fn string_count(self) -> usize {
        self.string_count
    }

    pub const fn instrument(self) -> Instrument {
        self.instrument
    }

    pub fn notes(&self) -> &[NoteName] {
        &self.notes[..self.string_count]
    }

    pub(crate) const fn note(self, string: usize) -> NoteName {
        self.notes[string]
    }

    pub(crate) const fn absolute_pitch(self, string: usize, fret: u8) -> i16 {
        self.open_pitches[string] + fret as i16
    }

    pub(crate) const fn pitch_at(self, string: usize, fret: u8) -> PitchClass {
        PitchClass::new(self.absolute_pitch(string, fret))
    }

    pub(crate) const fn prefers_root_bass(self) -> bool {
        !(self.string_count > 1 && self.open_pitches[0] > self.open_pitches[1])
    }
}

impl Default for GuitarTuning {
    fn default() -> Self {
        STANDARD_TUNING
    }
}

impl fmt::Display for GuitarTuning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, note) in self.notes().iter().enumerate() {
            if idx > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{note}")?;
        }
        Ok(())
    }
}

impl Serialize for Tuning {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Tuning", 3)?;
        state.serialize_field("instrument", &self.instrument)?;
        state.serialize_field("notes", self.notes())?;
        state.serialize_field("open_pitches", &self.open_pitches[..self.string_count])?;
        state.end()
    }
}

pub const STANDARD_TUNING: Tuning = Tuning::new(STANDARD_TUNING_NOTES);
pub const STANDARD_7_STRING_TUNING: Tuning = Tuning::new_guitar7(STANDARD_7_STRING_TUNING_NOTES);
pub const STANDARD_8_STRING_TUNING: Tuning = Tuning::new_guitar8(STANDARD_8_STRING_TUNING_NOTES);
pub const STANDARD_UKULELE_TUNING: Tuning = Tuning::new_ukulele(STANDARD_UKULELE_TUNING_NOTES);
pub const UKULELE_TUNING: Tuning = STANDARD_UKULELE_TUNING;

const fn open_pitches_for(
    notes: [NoteName; MAX_STRING_COUNT],
    string_count: usize,
    instrument: Instrument,
) -> [i16; MAX_STRING_COUNT] {
    if matches!(instrument, Instrument::Ukulele)
        && string_count == UKULELE_STRING_COUNT
        && is_reentrant_ukulele_pattern(notes)
    {
        reentrant_ukulele_open_pitches(notes)
    } else {
        ascending_open_pitches(notes, string_count)
    }
}

const fn ascending_open_pitches(
    notes: [NoteName; MAX_STRING_COUNT],
    string_count: usize,
) -> [i16; MAX_STRING_COUNT] {
    let mut out = [0i16; MAX_STRING_COUNT];
    let mut idx = 0usize;
    let mut previous = i16::MIN;
    while idx < string_count {
        let mut pitch = notes[idx].pitch_class().value() as i16;
        while pitch <= previous {
            pitch += 12;
        }
        out[idx] = pitch;
        previous = pitch;
        idx += 1;
    }
    out
}

const fn reentrant_ukulele_open_pitches(
    notes: [NoteName; MAX_STRING_COUNT],
) -> [i16; MAX_STRING_COUNT] {
    let second = notes[1].pitch_class().value() as i16;
    let third = lift_above(notes[2].pitch_class().value() as i16, second);
    let first = lift_above(notes[0].pitch_class().value() as i16, third);
    let fourth = lift_above(notes[3].pitch_class().value() as i16, first);
    [first, second, third, fourth, 0, 0, 0, 0]
}

const fn lift_above(mut pitch: i16, floor: i16) -> i16 {
    while pitch <= floor {
        pitch += 12;
    }
    pitch
}

const fn is_reentrant_ukulele_pattern(notes: [NoteName; MAX_STRING_COUNT]) -> bool {
    let first = notes[0].pitch_class();
    notes[1].pitch_class().value() == first.transpose(5).value()
        && notes[2].pitch_class().value() == first.transpose(9).value()
        && notes[3].pitch_class().value() == first.transpose(2).value()
}

fn has_tuning_separators(input: &str) -> bool {
    input
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, ',' | ';' | '/' | '-'))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct ParsedTuning {
    notes: [NoteName; MAX_STRING_COUNT],
    open_pitches: Option<[i16; MAX_STRING_COUNT]>,
    string_count: usize,
}

fn parse_separated_tuning(
    input: &str,
    expected: Option<usize>,
) -> Result<ParsedTuning, ChordsmithError> {
    let mut notes = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
    let mut open_pitches = [0i16; MAX_STRING_COUNT];
    let mut count = 0usize;
    let mut octave_count = 0usize;
    for part in input
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '/' | '-'))
        .filter(|part| !part.is_empty())
    {
        if count >= MAX_STRING_COUNT {
            count += 1;
            continue;
        }
        let parsed = parse_tuning_note(part)?;
        notes[count] = parsed.note;
        if let Some(pitch) = parsed.open_pitch {
            open_pitches[count] = pitch;
            octave_count += 1;
        }
        count += 1;
    }

    validate_string_count(count, expected, "tuning notes")?;
    validate_tuning_octaves(count, octave_count)?;
    Ok(ParsedTuning {
        notes,
        open_pitches: if octave_count == 0 {
            None
        } else {
            Some(open_pitches)
        },
        string_count: count,
    })
}

fn parse_compact_tuning(
    input: &str,
    expected: Option<usize>,
) -> Result<ParsedTuning, ChordsmithError> {
    let mut notes = [NoteName::const_new(NoteLetter::C, 0); MAX_STRING_COUNT];
    let mut open_pitches = [0i16; MAX_STRING_COUNT];
    let mut count = 0usize;
    let mut octave_count = 0usize;
    let mut rest = input;
    while !rest.is_empty() {
        let (note, next) = parse_note_prefix(rest)?;
        if next.len() == rest.len() {
            return Err(ChordsmithError::new(format!("invalid tuning '{input}'")));
        }
        if count < MAX_STRING_COUNT {
            notes[count] = note;
            let (octave, after_octave) = parse_optional_octave(next)?;
            if let Some(octave) = octave {
                open_pitches[count] = absolute_note_pitch(note, octave);
                octave_count += 1;
            }
            count += 1;
            rest = after_octave;
        } else {
            count += 1;
            rest = next;
        }
    }

    validate_string_count(count, expected, "tuning notes")?;
    validate_tuning_octaves(count, octave_count)?;
    Ok(ParsedTuning {
        notes,
        open_pitches: if octave_count == 0 {
            None
        } else {
            Some(open_pitches)
        },
        string_count: count,
    })
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct ParsedTuningNote {
    note: NoteName,
    open_pitch: Option<i16>,
}

fn parse_tuning_note(input: &str) -> Result<ParsedTuningNote, ChordsmithError> {
    let (note, rest) = parse_note_prefix(input)?;
    let (octave, rest) = parse_optional_octave(rest)?;
    if !rest.is_empty() {
        return Err(ChordsmithError::new(format!(
            "invalid tuning note '{input}'"
        )));
    }
    Ok(ParsedTuningNote {
        note,
        open_pitch: octave.map(|octave| absolute_note_pitch(note, octave)),
    })
}

fn parse_optional_octave(input: &str) -> Result<(Option<i8>, &str), ChordsmithError> {
    let end = input
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()
        .unwrap_or(0);
    if end == 0 {
        return Ok((None, input));
    }
    let octave = input[..end]
        .parse::<i8>()
        .map_err(|_| ChordsmithError::new(format!("invalid tuning octave '{}'", &input[..end])))?;
    Ok((Some(octave), &input[end..]))
}

const fn absolute_note_pitch(note: NoteName, octave: i8) -> i16 {
    octave as i16 * 12 + note.pitch_class().value() as i16
}

fn validate_tuning_octaves(count: usize, octave_count: usize) -> Result<(), ChordsmithError> {
    if octave_count == 0 || octave_count == count {
        Ok(())
    } else {
        Err(ChordsmithError::new(
            "invalid tuning: provide octaves for every note or for none",
        ))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct PitchSet {
    pub(crate) bits: u16,
}

impl PitchSet {
    pub(crate) const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub(crate) const fn all() -> Self {
        Self { bits: 0x0fff }
    }

    pub(crate) fn insert(&mut self, pitch: PitchClass) {
        self.bits |= 1u16 << pitch.value();
    }

    pub(crate) const fn with(mut self, pitch: PitchClass) -> Self {
        self.bits |= 1u16 << pitch.value();
        self
    }

    pub(crate) const fn contains(self, pitch: PitchClass) -> bool {
        self.bits & (1u16 << pitch.value()) != 0
    }

    pub(crate) const fn difference(self, other: Self) -> Self {
        Self {
            bits: self.bits & !other.bits,
        }
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub(crate) fn len(self) -> usize {
        self.bits.count_ones() as usize
    }

    pub(crate) fn is_empty(self) -> bool {
        self.bits == 0
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = PitchClass> {
        (0u8..12).filter_map(move |value| {
            let pitch = PitchClass(value);
            if self.contains(pitch) {
                Some(pitch)
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Fingering {
    pub(crate) frets: [Option<u8>; MAX_STRING_COUNT],
    pub(crate) string_count: usize,
}

impl Fingering {
    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        Self::parse_with_string_count(input, GUITAR_STRING_COUNT)
    }

    pub fn parse_with_string_count(
        input: &str,
        string_count: usize,
    ) -> Result<Self, ChordsmithError> {
        validate_exact_string_count(string_count)?;
        let trimmed = input.trim();
        if trimmed.contains('-') {
            parse_dashed_fingering(trimmed, string_count)
        } else {
            parse_compact_fingering(trimmed, string_count)
        }
    }

    pub const fn string_count(&self) -> usize {
        self.string_count
    }

    pub fn frets(&self) -> &[Option<u8>] {
        &self.frets[..self.string_count]
    }

    pub fn compact(&self) -> String {
        let has_double_digits = self.frets().iter().flatten().any(|fret| *fret > 9);
        if has_double_digits {
            self.dashed()
        } else {
            self.frets()
                .iter()
                .map(|fret| match fret {
                    Some(value) => value.to_string(),
                    None => "x".to_owned(),
                })
                .collect::<Vec<_>>()
                .join("")
        }
    }

    pub fn dashed(&self) -> String {
        self.frets()
            .iter()
            .map(|fret| match fret {
                Some(value) => value.to_string(),
                None => "x".to_owned(),
            })
            .collect::<Vec<_>>()
            .join("-")
    }
}

fn parse_compact_fingering(input: &str, string_count: usize) -> Result<Fingering, ChordsmithError> {
    let count = input.chars().count();
    validate_count(count, Some(string_count), "strings")?;

    let mut frets = [None; MAX_STRING_COUNT];
    for (idx, ch) in input.chars().enumerate() {
        frets[idx] = match ch {
            'x' | 'X' => None,
            '0'..='9' => ch
                .to_digit(10)
                .and_then(|value| u8::try_from(value).ok())
                .map(Some)
                .ok_or_else(|| ChordsmithError::new(format!("invalid fret '{ch}'")))?,
            _ => return Err(ChordsmithError::new(format!("invalid fret '{ch}'"))),
        };
    }

    Ok(Fingering {
        frets,
        string_count,
    })
}

fn parse_dashed_fingering(input: &str, string_count: usize) -> Result<Fingering, ChordsmithError> {
    let mut frets = [None; MAX_STRING_COUNT];
    let parts = input.split('-').collect::<Vec<_>>();
    validate_count(parts.len(), Some(string_count), "strings")?;

    for (idx, part) in parts.iter().enumerate() {
        let token = part.trim();
        frets[idx] = if token.eq_ignore_ascii_case("x") {
            None
        } else {
            let fret = token
                .parse::<u8>()
                .map_err(|_| ChordsmithError::new(format!("invalid fret '{token}'")))?;
            if fret > MAX_STANDARD_FRET {
                return Err(ChordsmithError::new(format!(
                    "invalid fret '{fret}': standard guitar range is 0..={MAX_STANDARD_FRET}"
                )));
            }
            Some(fret)
        };
    }

    Ok(Fingering {
        frets,
        string_count,
    })
}

fn validate_exact_string_count(string_count: usize) -> Result<(), ChordsmithError> {
    if is_supported_string_count(string_count) {
        Ok(())
    } else {
        Err(ChordsmithError::new(format!(
            "expected 4, 6, 7, or 8 strings, got {string_count}"
        )))
    }
}

fn validate_string_count(
    count: usize,
    expected: Option<usize>,
    label: &str,
) -> Result<(), ChordsmithError> {
    validate_count(count, expected, label)
}

fn validate_count(
    count: usize,
    expected: Option<usize>,
    label: &str,
) -> Result<(), ChordsmithError> {
    match expected {
        Some(expected) if count != expected => Err(ChordsmithError::new(format!(
            "expected {expected} {label}, got {count}"
        ))),
        Some(_) => Ok(()),
        None if is_supported_string_count(count) => Ok(()),
        None => Err(ChordsmithError::new(format!(
            "expected 4, 6, 7, or 8 {label}, got {count}"
        ))),
    }
}

const fn is_supported_string_count(count: usize) -> bool {
    matches!(
        count,
        UKULELE_STRING_COUNT | GUITAR_STRING_COUNT | GUITAR7_STRING_COUNT | GUITAR8_STRING_COUNT
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlayedNote {
    pub string: usize,
    pub fret: u8,
    pub note: NoteName,
    pub pitch_class: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlayedFingering {
    pub(crate) notes: Vec<PlayedNote>,
    pub(crate) set: PitchSet,
    pub(crate) bass: Option<PitchClass>,
}

pub(crate) fn play_fingering(fingering: &Fingering, tuning: GuitarTuning) -> PlayedFingering {
    let mut notes = Vec::new();
    let mut set = PitchSet::empty();
    let mut bass = None;
    let mut bass_pitch = i16::MAX;

    for (string, fret) in fingering.frets().iter().enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let absolute_pitch = tuning.absolute_pitch(string, *fret);
        let pitch = PitchClass::new(absolute_pitch);
        set.insert(pitch);
        if absolute_pitch < bass_pitch {
            bass_pitch = absolute_pitch;
            bass = Some(pitch);
        }
        let note = NoteName::simple_for_pitch(pitch, false);
        notes.push(PlayedNote {
            string,
            fret: *fret,
            note,
            pitch_class: pitch.value(),
        });
    }

    PlayedFingering { notes, set, bass }
}
