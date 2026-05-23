use std::cmp::Ordering;
use std::fmt;

use serde::Serialize;

pub const STRING_COUNT: usize = 6;
pub const DEFAULT_MAX_FRET: u8 = 12;
pub const DEFAULT_MAX_SPAN: u8 = 4;
pub const DEFAULT_LIMIT: usize = 15;
pub const MAX_STANDARD_FRET: u8 = 24;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChordsmithError {
    message: String,
}

impl ChordsmithError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ChordsmithError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChordsmithError {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub struct PitchClass(u8);

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
    fn from_ascii(ch: char) -> Option<Self> {
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

    const fn base_pitch(self) -> i16 {
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

    const fn index(self) -> usize {
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

    fn advance(self, steps: u8) -> Self {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub struct NoteName {
    pub letter: NoteLetter,
    pub accidental: i8,
}

impl NoteName {
    pub const fn new(letter: NoteLetter, accidental: i8) -> Self {
        Self { letter, accidental }
    }

    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        let (note, rest) = parse_note_prefix(input)?;
        if rest.is_empty() {
            Ok(note)
        } else {
            Err(ChordsmithError::new(format!("invalid note '{input}'")))
        }
    }

    pub const fn pitch_class(self) -> PitchClass {
        PitchClass::new(self.letter.base_pitch() + self.accidental as i16)
    }

    pub fn spell_for_pitch(letter: NoteLetter, pitch: PitchClass) -> Self {
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
        f.write_str(&self.letter.char().to_string())?;
        match self.accidental.cmp(&0) {
            Ordering::Greater => {
                for _ in 0..self.accidental {
                    f.write_str("#")?;
                }
            }
            Ordering::Less => {
                for _ in 0..self.accidental.abs() {
                    f.write_str("b")?;
                }
            }
            Ordering::Equal => {}
        }
        Ok(())
    }
}

const SIMPLE_SHARP_NAMES: [NoteName; 12] = [
    NoteName::new(NoteLetter::C, 0),
    NoteName::new(NoteLetter::C, 1),
    NoteName::new(NoteLetter::D, 0),
    NoteName::new(NoteLetter::D, 1),
    NoteName::new(NoteLetter::E, 0),
    NoteName::new(NoteLetter::F, 0),
    NoteName::new(NoteLetter::F, 1),
    NoteName::new(NoteLetter::G, 0),
    NoteName::new(NoteLetter::G, 1),
    NoteName::new(NoteLetter::A, 0),
    NoteName::new(NoteLetter::A, 1),
    NoteName::new(NoteLetter::B, 0),
];

const SIMPLE_FLAT_NAMES: [NoteName; 12] = [
    NoteName::new(NoteLetter::C, 0),
    NoteName::new(NoteLetter::D, -1),
    NoteName::new(NoteLetter::D, 0),
    NoteName::new(NoteLetter::E, -1),
    NoteName::new(NoteLetter::E, 0),
    NoteName::new(NoteLetter::F, 0),
    NoteName::new(NoteLetter::G, -1),
    NoteName::new(NoteLetter::G, 0),
    NoteName::new(NoteLetter::A, -1),
    NoteName::new(NoteLetter::A, 0),
    NoteName::new(NoteLetter::B, -1),
    NoteName::new(NoteLetter::B, 0),
];

pub const STANDARD_TUNING: [NoteName; STRING_COUNT] = [
    NoteName::new(NoteLetter::E, 0),
    NoteName::new(NoteLetter::A, 0),
    NoteName::new(NoteLetter::D, 0),
    NoteName::new(NoteLetter::G, 0),
    NoteName::new(NoteLetter::B, 0),
    NoteName::new(NoteLetter::E, 0),
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct PitchSet {
    bits: u16,
}

impl PitchSet {
    const fn empty() -> Self {
        Self { bits: 0 }
    }

    const fn all() -> Self {
        Self { bits: 0x0fff }
    }

    fn insert(&mut self, pitch: PitchClass) {
        self.bits |= 1u16 << pitch.value();
    }

    const fn contains(self, pitch: PitchClass) -> bool {
        self.bits & (1u16 << pitch.value()) != 0
    }

    const fn is_subset_of(self, other: Self) -> bool {
        self.bits & !other.bits == 0
    }

    const fn difference(self, other: Self) -> Self {
        Self {
            bits: self.bits & !other.bits,
        }
    }

    const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    fn len(self) -> usize {
        self.bits.count_ones() as usize
    }

    fn is_empty(self) -> bool {
        self.bits == 0
    }

    fn iter(self) -> impl Iterator<Item = PitchClass> {
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
    frets: [Option<u8>; STRING_COUNT],
}

impl Fingering {
    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        let trimmed = input.trim();
        if trimmed.contains('-') {
            parse_dashed_fingering(trimmed)
        } else {
            parse_compact_fingering(trimmed)
        }
    }

    pub const fn frets(&self) -> [Option<u8>; STRING_COUNT] {
        self.frets
    }

    pub fn compact(&self) -> String {
        let has_double_digits = self.frets.iter().flatten().any(|fret| *fret > 9);
        if has_double_digits {
            self.dashed()
        } else {
            self.frets
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
        self.frets
            .iter()
            .map(|fret| match fret {
                Some(value) => value.to_string(),
                None => "x".to_owned(),
            })
            .collect::<Vec<_>>()
            .join("-")
    }
}

fn parse_compact_fingering(input: &str) -> Result<Fingering, ChordsmithError> {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() != STRING_COUNT {
        return Err(ChordsmithError::new(format!(
            "expected {STRING_COUNT} strings, got {}",
            chars.len()
        )));
    }

    let mut frets = [None; STRING_COUNT];
    for (idx, ch) in chars.into_iter().enumerate() {
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

    Ok(Fingering { frets })
}

fn parse_dashed_fingering(input: &str) -> Result<Fingering, ChordsmithError> {
    let parts = input.split('-').collect::<Vec<_>>();
    if parts.len() != STRING_COUNT {
        return Err(ChordsmithError::new(format!(
            "expected {STRING_COUNT} strings, got {}",
            parts.len()
        )));
    }

    let mut frets = [None; STRING_COUNT];
    for (idx, part) in parts.into_iter().enumerate() {
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

    Ok(Fingering { frets })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlayedNote {
    pub string: usize,
    pub fret: u8,
    pub note: String,
    pub pitch_class: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlayedFingering {
    notes: Vec<PlayedNote>,
    set: PitchSet,
    bass: Option<PitchClass>,
}

fn play_fingering(fingering: &Fingering, tuning: [NoteName; STRING_COUNT]) -> PlayedFingering {
    let mut notes = Vec::new();
    let mut set = PitchSet::empty();
    let mut bass = None;

    for (string, fret) in fingering.frets.iter().enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = tuning[string].pitch_class().transpose(i16::from(*fret));
        set.insert(pitch);
        if bass.is_none() {
            bass = Some(pitch);
        }
        let note = NoteName::simple_for_pitch(pitch, false).to_string();
        notes.push(PlayedNote {
            string,
            fret: *fret,
            note,
            pitch_class: pitch.value(),
        });
    }

    PlayedFingering { notes, set, bass }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum Quality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Sus2,
    Sus4,
    Power,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum Seventh {
    None,
    Minor,
    Major,
    Diminished,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub enum Extension {
    Ninth,
    Eleventh,
    Thirteenth,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct Alteration {
    pub degree: u8,
    pub accidental: i8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordSpec {
    pub quality: Quality,
    pub seventh: Seventh,
    pub extension: Option<Extension>,
    pub sixth: bool,
    pub alt: bool,
    pub adds: Vec<u8>,
    pub alterations: Vec<Alteration>,
    pub omissions: Vec<u8>,
}

impl Default for ChordSpec {
    fn default() -> Self {
        Self {
            quality: Quality::Major,
            seventh: Seventh::None,
            extension: None,
            sixth: false,
            alt: false,
            adds: Vec::new(),
            alterations: Vec::new(),
            omissions: Vec::new(),
        }
    }
}

impl ChordSpec {
    pub fn suffix(&self) -> String {
        if self.quality == Quality::Power {
            return "5".to_owned();
        }
        if self.alt {
            return "alt".to_owned();
        }

        let mut out = String::new();
        let special_half_dim = self.quality == Quality::Minor
            && self.seventh == Seventh::Minor
            && self.extension.is_none()
            && !self.sixth
            && self.adds.is_empty()
            && self.alterations
                == [Alteration {
                    degree: 5,
                    accidental: -1,
                }];

        if special_half_dim {
            return "m7b5".to_owned();
        }

        match self.quality {
            Quality::Major => {}
            Quality::Minor => out.push('m'),
            Quality::Diminished => out.push_str("dim"),
            Quality::Augmented => out.push_str("aug"),
            Quality::Sus2 => out.push_str("sus2"),
            Quality::Sus4 => out.push_str("sus4"),
            Quality::Power => {}
        }

        if self.sixth {
            out.push('6');
            if self.adds.contains(&9) {
                out.push_str("/9");
            }
        }

        match self.extension {
            Some(Extension::Ninth) => push_extension(&mut out, self, "9"),
            Some(Extension::Eleventh) => push_extension(&mut out, self, "11"),
            Some(Extension::Thirteenth) => push_extension(&mut out, self, "13"),
            None => match self.seventh {
                Seventh::None => {}
                Seventh::Minor => out.push('7'),
                Seventh::Major => {
                    if self.quality == Quality::Minor {
                        out.push_str("(maj7)");
                    } else {
                        out.push_str("maj7");
                    }
                }
                Seventh::Diminished => out.push('7'),
            },
        }

        for add in &self.adds {
            if self.sixth && *add == 9 {
                continue;
            }
            out.push_str("add");
            out.push_str(&add.to_string());
        }

        for alteration in &self.alterations {
            out.push_str(alteration_prefix(alteration.accidental));
            out.push_str(&alteration.degree.to_string());
        }

        for omission in &self.omissions {
            out.push_str("no");
            out.push_str(&omission.to_string());
        }

        out
    }
}

fn push_extension(out: &mut String, spec: &ChordSpec, text: &str) {
    if spec.seventh == Seventh::Major && spec.quality == Quality::Minor {
        out.push_str("(maj");
        out.push_str(text);
        out.push(')');
        return;
    }

    match spec.seventh {
        Seventh::Major if spec.quality != Quality::Major => out.push_str("maj"),
        Seventh::Major if spec.quality == Quality::Major && out.is_empty() => out.push_str("maj"),
        Seventh::Major => out.push_str("maj"),
        Seventh::Diminished | Seventh::Minor | Seventh::None => {}
    }
    out.push_str(text);
}

fn alteration_prefix(accidental: i8) -> &'static str {
    match accidental.cmp(&0) {
        Ordering::Less => "b",
        Ordering::Equal => "",
        Ordering::Greater => "#",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordSymbol {
    pub root: NoteName,
    pub spec: ChordSpec,
    pub bass: Option<NoteName>,
}

impl ChordSymbol {
    pub fn parse(input: &str) -> Result<Self, ChordsmithError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ChordsmithError::new("empty chord symbol"));
        }

        let (root, rest) = parse_note_prefix(trimmed)?;
        let (descriptor, bass) = split_bass(rest)?;
        let spec = parse_descriptor(descriptor)?;
        Ok(Self { root, spec, bass })
    }

    pub fn formula(&self) -> ChordFormula {
        ChordFormula::from_parts(self.root, &self.spec)
    }

    pub fn name(&self) -> String {
        let mut out = format!("{}{}", self.root, self.spec.suffix());
        if let Some(bass) = self.bass {
            out.push('/');
            out.push_str(&bass.to_string());
        }
        out
    }
}

fn parse_note_prefix(input: &str) -> Result<(NoteName, &str), ChordsmithError> {
    let mut chars = input.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(ChordsmithError::new("empty note"));
    };
    let Some(letter) = NoteLetter::from_ascii(first) else {
        return Err(ChordsmithError::new(format!("invalid note root '{first}'")));
    };

    let mut accidental = 0i8;
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        match ch {
            '#' => {
                accidental += 1;
                end = idx + ch.len_utf8();
            }
            'b' => {
                accidental -= 1;
                end = idx + ch.len_utf8();
            }
            _ => break,
        }
    }

    Ok((NoteName { letter, accidental }, &input[end..]))
}

fn split_bass(rest: &str) -> Result<(&str, Option<NoteName>), ChordsmithError> {
    let Some(idx) = rest.rfind('/') else {
        return Ok((rest, None));
    };
    let after = &rest[idx + 1..];
    let Some(first) = after.chars().next() else {
        return Err(ChordsmithError::new("slash chord is missing bass note"));
    };
    if NoteLetter::from_ascii(first).is_none() {
        return Ok((rest, None));
    }
    let bass = NoteName::parse(after)?;
    Ok((&rest[..idx], Some(bass)))
}

fn parse_descriptor(input: &str) -> Result<ChordSpec, ChordsmithError> {
    let mut text = input
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '(' && *ch != ')')
        .collect::<String>();
    text = text.replace("6/9", "69");

    let mut spec = ChordSpec::default();
    let mut rest = text.as_str();

    if !starts_with_major_extension(rest)
        && let Some(next) = take_prefix(rest, &["maj", "Maj", "MAJ", "M"])
    {
        spec.quality = Quality::Major;
        rest = next;
    } else if !starts_with_major_extension(rest)
        && let Some(next) = take_prefix(rest, &["min", "Min", "MIN", "m", "-"])
    {
        spec.quality = Quality::Minor;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["dim", "Dim", "DIM", "o", "°"]) {
        spec.quality = Quality::Diminished;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["aug", "Aug", "AUG", "+"]) {
        spec.quality = Quality::Augmented;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["sus2"]) {
        spec.quality = Quality::Sus2;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["sus4", "sus", "Sus"]) {
        spec.quality = Quality::Sus4;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["5"]) {
        spec.quality = Quality::Power;
        rest = next;
    }

    while !rest.is_empty() {
        if let Some(next) = take_prefix(rest, &["ø", "Ø"]) {
            spec.quality = Quality::Minor;
            spec.seventh = Seventh::Minor;
            upsert_alteration(
                &mut spec.alterations,
                Alteration {
                    degree: 5,
                    accidental: -1,
                },
            );
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["alt", "Alt", "ALT"]) {
            spec.alt = true;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["maj13", "Maj13", "M13", "Δ13", "△13"]) {
            spec.seventh = Seventh::Major;
            spec.extension = Some(Extension::Thirteenth);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["maj11", "Maj11", "M11", "Δ11", "△11"]) {
            spec.seventh = Seventh::Major;
            spec.extension = Some(Extension::Eleventh);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["maj9", "Maj9", "M9", "Δ9", "△9"]) {
            spec.seventh = Seventh::Major;
            spec.extension = Some(Extension::Ninth);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["maj7", "Maj7", "M7", "Δ7", "△7", "Δ", "△"])
        {
            spec.seventh = Seventh::Major;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add13"]) {
            push_unique(&mut spec.adds, 13);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add11", "add4"]) {
            push_unique(&mut spec.adds, 11);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add9", "add2"]) {
            push_unique(&mut spec.adds, 9);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit5", "no5"]) {
            push_unique(&mut spec.omissions, 5);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit3", "no3"]) {
            push_unique(&mut spec.omissions, 3);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit1", "no1"]) {
            push_unique(&mut spec.omissions, 1);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["13"]) {
            spec.seventh = default_seventh_for_extension(spec.quality);
            spec.extension = Some(Extension::Thirteenth);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["11"]) {
            spec.seventh = default_seventh_for_extension(spec.quality);
            spec.extension = Some(Extension::Eleventh);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["9"]) {
            spec.seventh = default_seventh_for_extension(spec.quality);
            spec.extension = Some(Extension::Ninth);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["69"]) {
            spec.sixth = true;
            push_unique(&mut spec.adds, 9);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["7"]) {
            spec.seventh = if spec.quality == Quality::Diminished {
                Seventh::Diminished
            } else {
                Seventh::Minor
            };
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["6"]) {
            spec.sixth = true;
            rest = next;
        } else if let Some((alteration, next)) = take_alteration(rest) {
            upsert_alteration(&mut spec.alterations, alteration);
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["/"]) {
            rest = next;
        } else {
            return Err(ChordsmithError::new(format!(
                "unknown chord descriptor near '{rest}'"
            )));
        }
    }

    validate_descriptor(&spec)?;
    normalize_spec(&mut spec);
    Ok(spec)
}

fn validate_descriptor(spec: &ChordSpec) -> Result<(), ChordsmithError> {
    if spec.quality == Quality::Power
        && (spec.seventh != Seventh::None
            || spec.extension.is_some()
            || spec.sixth
            || spec.alt
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
    {
        return Err(ChordsmithError::new(
            "invalid chord descriptor: power chords cannot be combined with other chord tones",
        ));
    }

    if spec.alt
        && (spec.seventh == Seventh::Major
            || spec.extension.is_some()
            || spec.sixth
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
    {
        return Err(ChordsmithError::new(
            "invalid chord descriptor: alt is already a complete altered dominant set",
        ));
    }

    Ok(())
}

fn take_prefix<'a>(input: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes
        .iter()
        .find_map(|prefix| input.strip_prefix(prefix))
}

fn starts_with_major_extension(input: &str) -> bool {
    [
        "maj13", "Maj13", "M13", "Δ13", "△13", "maj11", "Maj11", "M11", "Δ11", "△11", "maj9",
        "Maj9", "M9", "Δ9", "△9", "maj7", "Maj7", "M7", "Δ7", "△7", "Δ", "△",
    ]
    .iter()
    .any(|prefix| input.starts_with(prefix))
}

fn take_alteration(input: &str) -> Option<(Alteration, &str)> {
    let (accidental, rest) = if let Some(rest) = input.strip_prefix('b') {
        (-1, rest)
    } else if let Some(rest) = input.strip_prefix('#') {
        (1, rest)
    } else if let Some(rest) = input.strip_prefix('+') {
        (1, rest)
    } else {
        return None;
    };

    for degree in [13u8, 11, 9, 5] {
        let text = degree.to_string();
        if let Some(next) = rest.strip_prefix(&text) {
            return Some((Alteration { degree, accidental }, next));
        }
    }
    None
}

fn default_seventh_for_extension(quality: Quality) -> Seventh {
    if quality == Quality::Diminished {
        Seventh::Diminished
    } else {
        Seventh::Minor
    }
}

fn push_unique(values: &mut Vec<u8>, value: u8) {
    if !values.contains(&value) {
        values.push(value);
        values.sort_unstable();
    }
}

fn upsert_alteration(values: &mut Vec<Alteration>, value: Alteration) {
    if values.contains(&value) {
        return;
    }

    values.push(value);
    values.sort_unstable();
}

fn normalize_spec(spec: &mut ChordSpec) {
    if spec.quality == Quality::Power {
        spec.seventh = Seventh::None;
        spec.extension = None;
        spec.sixth = false;
        spec.alt = false;
        spec.adds.clear();
        spec.alterations.clear();
        spec.omissions.clear();
        return;
    }

    if spec.alt {
        spec.quality = Quality::Major;
        spec.seventh = Seventh::Minor;
        return;
    }

    if spec.extension.is_some() {
        spec.sixth = false;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordTone {
    pub degree: u8,
    pub interval: String,
    pub semitones: i16,
    pub note: String,
    pub pitch_class: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordFormula {
    pub tones: Vec<ChordTone>,
}

impl ChordFormula {
    fn from_parts(root: NoteName, spec: &ChordSpec) -> Self {
        let tones = raw_tones_from_spec(spec)
            .into_iter()
            .map(|tone| {
                let pitch = root.pitch_class().transpose(tone.semitones);
                let letter = root.letter.advance(degree_letter_steps(tone.degree));
                let note = NoteName::spell_for_pitch(letter, pitch);
                ChordTone {
                    degree: tone.degree,
                    interval: interval_name(tone.degree, tone.semitones),
                    semitones: tone.semitones,
                    note: note.to_string(),
                    pitch_class: pitch.value(),
                }
            })
            .collect();

        Self { tones }
    }

    fn pitch_set(&self) -> PitchSet {
        let mut set = PitchSet::empty();
        for tone in &self.tones {
            set.insert(PitchClass(tone.pitch_class));
        }
        set
    }

    fn has_duplicate_pitch_classes(&self) -> bool {
        self.pitch_set().len() != self.tones.len()
    }

    fn tone_for_pitch(&self, pitch: PitchClass) -> Option<&ChordTone> {
        self.tones
            .iter()
            .find(|tone| tone.pitch_class == pitch.value())
    }
}

fn raw_tones_from_spec(spec: &ChordSpec) -> Vec<RawTone> {
    let mut raw = base_raw_tones(spec);

    for alteration in &spec.alterations {
        let natural = natural_semitones(alteration.degree);
        raw.retain(|tone| tone.degree != alteration.degree || tone.semitones != natural);
    }

    for alteration in &spec.alterations {
        let natural = natural_semitones(alteration.degree);
        let semitones = natural + i16::from(alteration.accidental);
        push_tone(&mut raw, RawTone::new(alteration.degree, semitones));
    }

    for omission in &spec.omissions {
        raw.retain(|tone| tone.degree != *omission);
    }

    normalize_raw_tones(raw)
}

fn base_raw_tones(spec: &ChordSpec) -> Vec<RawTone> {
    if spec.alt {
        return vec![
            RawTone::new(1, 0),
            RawTone::new(3, 4),
            RawTone::new(7, 10),
            RawTone::new(9, 13),
            RawTone::new(9, 15),
            RawTone::new(13, 20),
        ];
    }

    let mut raw = Vec::<RawTone>::new();
    raw.push(RawTone::new(1, 0));

    match spec.quality {
        Quality::Major => {
            raw.push(RawTone::new(3, 4));
            raw.push(RawTone::new(5, 7));
        }
        Quality::Minor => {
            raw.push(RawTone::new(3, 3));
            raw.push(RawTone::new(5, 7));
        }
        Quality::Diminished => {
            raw.push(RawTone::new(3, 3));
            raw.push(RawTone::new(5, 6));
        }
        Quality::Augmented => {
            raw.push(RawTone::new(3, 4));
            raw.push(RawTone::new(5, 8));
        }
        Quality::Sus2 => {
            raw.push(RawTone::new(2, 2));
            raw.push(RawTone::new(5, 7));
        }
        Quality::Sus4 => {
            raw.push(RawTone::new(4, 5));
            raw.push(RawTone::new(5, 7));
        }
        Quality::Power => {
            raw.push(RawTone::new(5, 7));
        }
    }

    if spec.sixth {
        upsert_tone(&mut raw, RawTone::new(6, 9));
    }

    match spec.seventh {
        Seventh::None => {}
        Seventh::Minor => upsert_tone(&mut raw, RawTone::new(7, 10)),
        Seventh::Major => upsert_tone(&mut raw, RawTone::new(7, 11)),
        Seventh::Diminished => upsert_tone(&mut raw, RawTone::new(7, 9)),
    }

    match spec.extension {
        Some(Extension::Ninth) => upsert_tone(&mut raw, RawTone::new(9, 14)),
        Some(Extension::Eleventh) => {
            upsert_tone(&mut raw, RawTone::new(9, 14));
            upsert_tone(&mut raw, RawTone::new(11, 17));
        }
        Some(Extension::Thirteenth) => {
            upsert_tone(&mut raw, RawTone::new(9, 14));
            upsert_tone(&mut raw, RawTone::new(13, 21));
        }
        None => {}
    }

    for add in &spec.adds {
        match *add {
            9 => upsert_tone(&mut raw, RawTone::new(9, 14)),
            11 => upsert_tone(&mut raw, RawTone::new(11, 17)),
            13 => upsert_tone(&mut raw, RawTone::new(13, 21)),
            _ => {}
        }
    }

    raw
}

fn normalize_raw_tones(mut raw: Vec<RawTone>) -> Vec<RawTone> {
    raw.sort_by_key(|tone| (degree_order(tone.degree), tone.semitones));
    raw.dedup_by_key(|tone| (tone.degree, tone.semitones));
    raw
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct RawTone {
    degree: u8,
    semitones: i16,
}

impl RawTone {
    const fn new(degree: u8, semitones: i16) -> Self {
        Self { degree, semitones }
    }
}

fn upsert_tone(tones: &mut Vec<RawTone>, tone: RawTone) {
    tones.retain(|item| item.degree != tone.degree);
    tones.push(tone);
}

fn push_tone(tones: &mut Vec<RawTone>, tone: RawTone) {
    if !tones.contains(&tone) {
        tones.push(tone);
    }
}

fn natural_semitones(degree: u8) -> i16 {
    match degree {
        1 => 0,
        2 => 2,
        3 => 4,
        4 => 5,
        5 => 7,
        6 => 9,
        7 => 11,
        9 => 14,
        11 => 17,
        13 => 21,
        _ => 0,
    }
}

fn degree_order(degree: u8) -> u8 {
    match degree {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        7 => 6,
        9 => 7,
        11 => 8,
        13 => 9,
        _ => degree,
    }
}

fn degree_letter_steps(degree: u8) -> u8 {
    match degree {
        1 => 0,
        2 | 9 => 1,
        3 => 2,
        4 | 11 => 3,
        5 => 4,
        6 | 13 => 5,
        7 => 6,
        _ => 0,
    }
}

fn interval_name(degree: u8, semitones: i16) -> String {
    let natural = natural_semitones(degree);
    let delta = semitones - natural;
    let prefix = match delta.cmp(&0) {
        Ordering::Less => "b".repeat(delta.unsigned_abs() as usize),
        Ordering::Equal => String::new(),
        Ordering::Greater => "#".repeat(delta.unsigned_abs() as usize),
    };
    format!("{prefix}{degree}")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Confidence {
    Exact,
    Omitted,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize)]
pub enum AnalysisClass {
    Primary,
    UsefulAlias,
    TheoreticalAlias,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordAnalysis {
    pub symbol: String,
    pub root: String,
    pub bass: String,
    pub notes: Vec<String>,
    pub intervals: Vec<String>,
    pub omissions: Vec<String>,
    pub confidence: Confidence,
    pub class: AnalysisClass,
    pub score: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IdentifyResult {
    pub fingering: String,
    pub notes: Vec<PlayedNote>,
    pub primary: Option<ChordAnalysis>,
    pub aliases: Vec<ChordAnalysis>,
}

pub fn identify(input: &str) -> Result<IdentifyResult, ChordsmithError> {
    let fingering = Fingering::parse(input)?;
    identify_fingering(&fingering)
}

pub fn identify_fingering(fingering: &Fingering) -> Result<IdentifyResult, ChordsmithError> {
    let played = play_fingering(fingering, STANDARD_TUNING);
    if played.set.is_empty() {
        return Ok(IdentifyResult {
            fingering: fingering.compact(),
            notes: Vec::new(),
            primary: None,
            aliases: Vec::new(),
        });
    }

    let Some(bass) = played.bass else {
        return Ok(IdentifyResult {
            fingering: fingering.compact(),
            notes: played.notes,
            primary: None,
            aliases: Vec::new(),
        });
    };

    let specs = candidate_specs();
    let mut analyses = Vec::new();
    for root in candidate_roots(played.set) {
        for spec in &specs {
            let formula = ChordFormula::from_parts(root, spec);
            if formula.has_duplicate_pitch_classes() {
                continue;
            }
            let formula_set = formula.pitch_set();
            if played.set == formula_set {
                analyses.push(build_analysis(
                    root,
                    spec.clone(),
                    &formula,
                    bass,
                    Vec::new(),
                    false,
                ));
                continue;
            }

            let missing = formula_set.difference(played.set);
            if played.set.is_subset_of(formula_set)
                && let Some(omissions) = inferred_omissions(missing, &formula)
            {
                analyses.push(build_analysis(
                    root,
                    spec.clone(),
                    &formula,
                    bass,
                    omissions,
                    true,
                ));
            }
        }
    }

    analyses.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then(left.symbol.cmp(&right.symbol))
    });
    analyses.dedup_by(|left, right| left.symbol == right.symbol);

    let primary_score = analyses.first().map(|analysis| analysis.score).unwrap_or(0);
    let primary = analyses.first().cloned().map(|mut analysis| {
        analysis.class = AnalysisClass::Primary;
        analysis
    });
    let notes = respell_played_notes(&played.notes, primary.as_ref());
    let aliases = analyses
        .into_iter()
        .skip(1)
        .map(|mut analysis| {
            analysis.class = classify_alias(&analysis, primary_score);
            analysis
        })
        .take(24)
        .collect();

    Ok(IdentifyResult {
        fingering: fingering.compact(),
        notes,
        primary,
        aliases,
    })
}

fn candidate_roots(set: PitchSet) -> Vec<NoteName> {
    let mut roots = Vec::new();
    for pitch in set.iter() {
        push_candidate_spellings(&mut roots, pitch);
    }
    for pitch in PitchSet::all().iter() {
        push_candidate_spellings(&mut roots, pitch);
    }
    roots
}

fn push_candidate_spellings(notes: &mut Vec<NoteName>, pitch: PitchClass) {
    let flat = NoteName::simple_for_pitch(pitch, true);
    let sharp = NoteName::simple_for_pitch(pitch, false);
    if prefer_flat_pitch(pitch) {
        push_unique_note(notes, flat);
        push_unique_note(notes, sharp);
    } else {
        push_unique_note(notes, sharp);
        push_unique_note(notes, flat);
    }
}

fn push_unique_note(notes: &mut Vec<NoteName>, note: NoteName) {
    if !notes.contains(&note) {
        notes.push(note);
    }
}

fn prefer_flat_pitch(pitch: PitchClass) -> bool {
    matches!(pitch.value(), 1 | 3 | 8 | 10)
}

fn respell_played_notes(notes: &[PlayedNote], primary: Option<&ChordAnalysis>) -> Vec<PlayedNote> {
    let Some(primary) = primary else {
        return notes.to_vec();
    };

    let mut spellings = vec![None::<String>; 12];
    for note in &primary.notes {
        if let Ok(parsed) = NoteName::parse(note) {
            spellings[usize::from(parsed.pitch_class().value())] = Some(note.clone());
        }
    }

    notes
        .iter()
        .map(|played| {
            let mut respelled = played.clone();
            if let Some(note) = &spellings[usize::from(played.pitch_class)] {
                respelled.note.clone_from(note);
            }
            respelled
        })
        .collect()
}

fn candidate_specs() -> Vec<ChordSpec> {
    let mut specs = Vec::new();
    for quality in [
        Quality::Major,
        Quality::Minor,
        Quality::Diminished,
        Quality::Augmented,
        Quality::Sus2,
        Quality::Sus4,
        Quality::Power,
    ] {
        let base = ChordSpec {
            quality,
            ..ChordSpec::default()
        };
        specs.push(base.clone());

        if quality == Quality::Power {
            continue;
        }

        if quality == Quality::Major {
            specs.push(ChordSpec {
                alt: true,
                ..ChordSpec::default()
            });
        }

        for add in [9u8, 11, 13] {
            let mut spec = base.clone();
            push_unique(&mut spec.adds, add);
            specs.push(spec);
        }

        if matches!(quality, Quality::Major | Quality::Minor) {
            let mut six = base.clone();
            six.sixth = true;
            specs.push(six.clone());
            push_unique(&mut six.adds, 9);
            specs.push(six);
        }

        for seventh in allowed_sevenths(quality) {
            let mut seventh_spec = base.clone();
            seventh_spec.seventh = seventh;
            specs.push(seventh_spec.clone());

            for extension in [Extension::Ninth, Extension::Eleventh, Extension::Thirteenth] {
                let mut extended = seventh_spec.clone();
                extended.extension = Some(extension);
                specs.push(extended.clone());
                for altered in alteration_sets(extension) {
                    let mut spec = extended.clone();
                    spec.alterations = altered;
                    specs.push(spec);
                }
            }

            for altered in alteration_sets(Extension::Ninth) {
                let mut spec = seventh_spec.clone();
                spec.alterations = altered;
                specs.push(spec);
            }
        }
    }

    let mut half_dim = ChordSpec {
        quality: Quality::Minor,
        seventh: Seventh::Minor,
        extension: None,
        sixth: false,
        alt: false,
        adds: Vec::new(),
        alterations: vec![Alteration {
            degree: 5,
            accidental: -1,
        }],
        omissions: Vec::new(),
    };
    specs.push(half_dim.clone());
    half_dim.extension = Some(Extension::Ninth);
    specs.push(half_dim);

    specs.retain(|spec| !has_redundant_alteration(spec) && !has_omitted_alteration(spec));
    specs.sort_by_key(ChordSpec::suffix);
    specs.dedup_by(|left, right| left == right);
    specs
}

fn allowed_sevenths(quality: Quality) -> Vec<Seventh> {
    match quality {
        Quality::Diminished => vec![Seventh::Diminished],
        Quality::Power => Vec::new(),
        _ => vec![Seventh::Minor, Seventh::Major],
    }
}

fn alteration_sets(_extension: Extension) -> Vec<Vec<Alteration>> {
    let alterations = [
        Alteration {
            degree: 5,
            accidental: -1,
        },
        Alteration {
            degree: 5,
            accidental: 1,
        },
        Alteration {
            degree: 9,
            accidental: -1,
        },
        Alteration {
            degree: 9,
            accidental: 1,
        },
        Alteration {
            degree: 11,
            accidental: 1,
        },
        Alteration {
            degree: 13,
            accidental: -1,
        },
    ];

    let mut options = Vec::new();
    for mask in 0..(1usize << alterations.len()) {
        let mut option = Vec::new();
        for (idx, alteration) in alterations.iter().enumerate() {
            if mask & (1usize << idx) != 0 {
                option.push(*alteration);
            }
        }
        options.push(option);
    }
    options
}

fn inferred_omissions(missing: PitchSet, formula: &ChordFormula) -> Option<Vec<String>> {
    if missing.is_empty() {
        return Some(Vec::new());
    }

    let mut omissions = Vec::new();
    for pitch in missing.iter() {
        let tone = formula.tone_for_pitch(pitch)?;
        if !can_infer_omission(tone, formula) {
            return None;
        }
        if formula.tones.iter().any(|other| {
            other.degree == tone.degree && !missing.contains(PitchClass(other.pitch_class))
        }) {
            return None;
        }
        push_unique(&mut omissions, tone.degree);
    }

    if omissions.is_empty() {
        return None;
    }

    Some(
        omissions
            .into_iter()
            .map(|degree| degree.to_string())
            .collect(),
    )
}

fn can_infer_omission(tone: &ChordTone, formula: &ChordFormula) -> bool {
    match tone.degree {
        1 => formula.tones.len() >= 4,
        3 => formula.tones.len() >= 3,
        5 => true,
        _ => false,
    }
}

fn build_analysis(
    root: NoteName,
    spec: ChordSpec,
    formula: &ChordFormula,
    bass: PitchClass,
    omissions: Vec<String>,
    omitted: bool,
) -> ChordAnalysis {
    let bass_name = formula
        .tone_for_pitch(bass)
        .map(|tone| tone.note.clone())
        .unwrap_or_else(|| NoteName::simple_for_pitch(bass, root.accidental < 0).to_string());

    let mut symbol = format!("{}{}", root, spec.suffix());
    if !omissions.is_empty() {
        let omission_text = omissions
            .iter()
            .map(|omission| format!("no{omission}"))
            .collect::<Vec<_>>()
            .join(",");
        symbol.push('(');
        symbol.push_str(&omission_text);
        symbol.push(')');
    }
    if bass != root.pitch_class() {
        symbol.push('/');
        symbol.push_str(&bass_name);
    }

    let mut score = analysis_score(root, &spec, formula, bass, omitted);
    for omission in &omissions {
        score += omission_score(omission);
    }

    ChordAnalysis {
        symbol,
        root: root.to_string(),
        bass: bass_name,
        notes: formula.tones.iter().map(|tone| tone.note.clone()).collect(),
        intervals: formula
            .tones
            .iter()
            .map(|tone| tone.interval.clone())
            .collect(),
        omissions,
        confidence: if omitted {
            Confidence::Omitted
        } else {
            Confidence::Exact
        },
        class: AnalysisClass::TheoreticalAlias,
        score,
    }
}

fn classify_alias(analysis: &ChordAnalysis, primary_score: u32) -> AnalysisClass {
    if analysis.score > primary_score.saturating_add(180) {
        return AnalysisClass::TheoreticalAlias;
    }

    if analysis.confidence == Confidence::Omitted {
        return if analysis.omissions == ["5"] {
            AnalysisClass::UsefulAlias
        } else {
            AnalysisClass::TheoreticalAlias
        };
    }

    if analysis.intervals.len() <= 4
        && analysis
            .intervals
            .iter()
            .all(|interval| is_stable_alias_interval(interval))
    {
        AnalysisClass::UsefulAlias
    } else {
        AnalysisClass::TheoreticalAlias
    }
}

fn is_stable_alias_interval(interval: &str) -> bool {
    matches!(
        interval,
        "1" | "2" | "b3" | "3" | "4" | "b5" | "5" | "#5" | "6" | "bb7" | "b7" | "7"
    )
}

fn omission_score(omission: &str) -> u32 {
    match omission {
        "1" => 500,
        "3" => 500,
        "5" => 80,
        _ => 200,
    }
}

fn analysis_score(
    root: NoteName,
    spec: &ChordSpec,
    formula: &ChordFormula,
    bass: PitchClass,
    omitted: bool,
) -> u32 {
    let mut score = 0u32;
    if bass != root.pitch_class() {
        score += 400;
    }
    score += u32::try_from(spec.suffix().len()).unwrap_or(100);
    score += u32::from(root.accidental.unsigned_abs()) * 8;
    score += root_spelling_penalty(root);
    score += u32::try_from(formula.tones.len()).unwrap_or(20) * 4;

    if omitted {
        score += 80;
    }
    if spec.extension.is_some() {
        score += 30;
    }
    if !spec.adds.is_empty() {
        score += 20;
    }
    if !spec.alterations.is_empty() {
        score += u32::try_from(spec.alterations.len()).unwrap_or(10) * 25;
    }
    if matches!(spec.quality, Quality::Major)
        && spec.seventh == Seventh::None
        && spec.extension.is_none()
    {
        score = score.saturating_sub(10);
    }
    score
}

fn root_spelling_penalty(root: NoteName) -> u32 {
    let pitch = root.pitch_class();
    if prefer_flat_pitch(pitch) && root.accidental > 0 {
        return 30;
    }
    if !prefer_flat_pitch(pitch) && root.accidental < 0 {
        return 30;
    }
    0
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct VoicingOptions {
    pub max_fret: u8,
    pub max_span: u8,
    pub limit: usize,
    pub all: bool,
}

impl Default for VoicingOptions {
    fn default() -> Self {
        Self {
            max_fret: DEFAULT_MAX_FRET,
            max_span: DEFAULT_MAX_SPAN,
            limit: DEFAULT_LIMIT,
            all: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Voicing {
    pub compact: String,
    pub dashed: String,
    pub frets: [Option<u8>; STRING_COUNT],
    pub notes: Vec<String>,
    pub omissions: Vec<String>,
    pub score: u32,
}

pub fn voicings(input: &str, options: VoicingOptions) -> Result<Vec<Voicing>, ChordsmithError> {
    if options.max_fret > MAX_STANDARD_FRET {
        return Err(ChordsmithError::new(format!(
            "invalid max_fret '{}': standard guitar range is 0..={MAX_STANDARD_FRET}",
            options.max_fret
        )));
    }
    if options.max_span > MAX_STANDARD_FRET {
        return Err(ChordsmithError::new(format!(
            "invalid max_span '{}': standard guitar range is 0..={MAX_STANDARD_FRET}",
            options.max_span
        )));
    }

    let chord = ChordSymbol::parse(input)?;
    let formula = chord.formula();
    reject_redundant_formula(&chord, &formula)?;
    let target = formula.pitch_set();
    let bass = chord
        .bass
        .map(NoteName::pitch_class)
        .or_else(|| (!chord.spec.omissions.contains(&1)).then_some(chord.root.pitch_class()));

    let mut per_string = Vec::new();
    for (string, open) in STANDARD_TUNING.iter().enumerate() {
        let mut choices = vec![None];
        for fret in 0..=options.max_fret {
            let pitch = open.pitch_class().transpose(i16::from(fret));
            if target.contains(pitch) {
                choices.push(Some(fret));
            }
        }
        choices.sort_by_key(|choice| choice_sort_key(*choice, string));
        per_string.push(choices);
    }
    let suffix_sets = suffix_pitch_sets(&per_string);

    let mut out = Vec::new();
    let mut frets = [None; STRING_COUNT];
    let search = VoicingSearch {
        per_string: &per_string,
        suffix_sets: &suffix_sets,
        target,
        bass,
        formula: &formula,
        options,
    };
    enumerate_voicings(&search, 0, &mut frets, &mut out);

    Ok(rank_voicings(out, options))
}

struct VoicingSearch<'a> {
    per_string: &'a [Vec<Option<u8>>],
    suffix_sets: &'a [PitchSet],
    target: PitchSet,
    bass: Option<PitchClass>,
    formula: &'a ChordFormula,
    options: VoicingOptions,
}

fn enumerate_voicings(
    search: &VoicingSearch<'_>,
    string_idx: usize,
    frets: &mut [Option<u8>; STRING_COUNT],
    out: &mut Vec<Voicing>,
) {
    if string_idx == STRING_COUNT {
        if let Some(voicing) = build_voicing(
            *frets,
            search.target,
            search.bass,
            search.formula,
            search.options,
        ) {
            out.push(voicing);
        }
        return;
    }

    if let Some(choices) = search.per_string.get(string_idx) {
        for choice in choices {
            frets[string_idx] = *choice;
            if !partial_voicing_can_complete(search, string_idx + 1, frets) {
                continue;
            }
            enumerate_voicings(search, string_idx + 1, frets, out);
        }
    }
}

fn choice_sort_key(choice: Option<u8>, string: usize) -> (u8, u8, usize) {
    match choice {
        Some(0) => (0, 0, string),
        Some(fret) => (1, fret, string),
        None => (2, 0, string),
    }
}

fn suffix_pitch_sets(per_string: &[Vec<Option<u8>>]) -> Vec<PitchSet> {
    let mut suffix = vec![PitchSet::empty(); STRING_COUNT + 1];
    for string in (0..STRING_COUNT).rev() {
        let mut set = suffix[string + 1];
        for fret in &per_string[string] {
            let Some(fret) = fret else {
                continue;
            };
            let pitch = STANDARD_TUNING[string]
                .pitch_class()
                .transpose(i16::from(*fret));
            set.insert(pitch);
        }
        suffix[string] = set;
    }
    suffix
}

fn partial_voicing_can_complete(
    search: &VoicingSearch<'_>,
    next_string: usize,
    frets: &[Option<u8>; STRING_COUNT],
) -> bool {
    if !partial_span_valid(frets, next_string, search.options.max_span) {
        return false;
    }

    if let Some(expected_bass) = search.bass
        && let Some(actual_bass) = partial_bass(frets, next_string)
        && actual_bass != expected_bass
    {
        return false;
    }

    let current = partial_pitch_set(frets, next_string);
    let available = current.union(search.suffix_sets[next_string]);
    let missing = search.target.difference(available);
    missing.is_empty() || can_omit_formula_fifth(missing, search.formula)
}

fn partial_span_valid(
    frets: &[Option<u8>; STRING_COUNT],
    next_string: usize,
    max_span: u8,
) -> bool {
    let non_open = frets
        .iter()
        .take(next_string)
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    if let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) {
        max.saturating_sub(*min) <= max_span
    } else {
        true
    }
}

fn partial_bass(frets: &[Option<u8>; STRING_COUNT], next_string: usize) -> Option<PitchClass> {
    frets
        .iter()
        .take(next_string)
        .enumerate()
        .find_map(|(string, fret)| {
            fret.map(|fret| {
                STANDARD_TUNING[string]
                    .pitch_class()
                    .transpose(i16::from(fret))
            })
        })
}

fn partial_pitch_set(frets: &[Option<u8>; STRING_COUNT], next_string: usize) -> PitchSet {
    let mut set = PitchSet::empty();
    for (string, fret) in frets.iter().take(next_string).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = STANDARD_TUNING[string]
            .pitch_class()
            .transpose(i16::from(*fret));
        set.insert(pitch);
    }
    set
}

fn build_voicing(
    frets: [Option<u8>; STRING_COUNT],
    target: PitchSet,
    expected_bass: Option<PitchClass>,
    formula: &ChordFormula,
    options: VoicingOptions,
) -> Option<Voicing> {
    let active = frets.iter().flatten().copied().collect::<Vec<_>>();
    if active.is_empty() {
        return None;
    }

    let non_open = active
        .iter()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    if let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max())
        && max.saturating_sub(*min) > options.max_span
    {
        return None;
    }

    let mut current = PitchSet::empty();
    let mut actual_bass = None;
    let mut notes = Vec::new();

    for (string, fret) in frets.iter().enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = STANDARD_TUNING[string]
            .pitch_class()
            .transpose(i16::from(*fret));
        current.insert(pitch);
        if actual_bass.is_none() {
            actual_bass = Some(pitch);
        }
        let note = formula
            .tone_for_pitch(pitch)
            .map(|tone| tone.note.clone())
            .unwrap_or_else(|| NoteName::simple_for_pitch(pitch, false).to_string());
        notes.push(note);
    }

    let missing = target.difference(current);
    if !missing.is_empty() && !can_omit_formula_fifth(missing, formula) {
        return None;
    }
    let omissions = voicing_omissions(missing, formula);
    if let Some(expected_bass) = expected_bass
        && actual_bass != Some(expected_bass)
    {
        return None;
    }

    let fingering = Fingering { frets };
    let score = voicing_score(&frets, expected_bass);
    Some(Voicing {
        compact: fingering.compact(),
        dashed: fingering.dashed(),
        frets,
        notes,
        omissions,
        score,
    })
}

fn voicing_omissions(missing: PitchSet, formula: &ChordFormula) -> Vec<String> {
    missing
        .iter()
        .filter_map(|pitch| formula.tone_for_pitch(pitch))
        .map(|tone| tone.interval.clone())
        .collect()
}

fn voicing_score(frets: &[Option<u8>; STRING_COUNT], expected_bass: Option<PitchClass>) -> u32 {
    let active = frets.iter().flatten().copied().collect::<Vec<_>>();
    if active.is_empty() {
        return u32::MAX;
    }

    let non_open = active
        .iter()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();

    let position_cost = position_cost(&non_open, active.contains(&0));
    let relative_cost = relative_fret_cost(&non_open);
    let span_cost = fret_span_cost(&non_open);
    let string_cost = active_string_cost(active.len());
    let internal_mute_cost = internal_mutes(frets) * 12;
    let jump_cost = adjacent_fret_jump_cost(frets);
    let duplicate_cost = duplicate_pitch_cost(frets);
    let high_open_cost = high_open_mix_cost(frets);
    let low_open_gap_cost = low_open_gap_cost(frets);

    let mut score = position_cost
        + relative_cost
        + span_cost
        + string_cost
        + internal_mute_cost
        + jump_cost
        + duplicate_cost
        + high_open_cost
        + low_open_gap_cost;
    score = score.saturating_sub(open_position_bonus(frets));
    score = score.saturating_sub(open_root_bass_bonus(frets, expected_bass));
    score = score.saturating_sub(open_bass_grip_bonus(frets));
    score = score.saturating_sub(closed_shape_bonus(frets));
    score
}

fn rank_voicings(mut voicings: Vec<Voicing>, options: VoicingOptions) -> Vec<Voicing> {
    voicings.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then(left.compact.cmp(&right.compact))
    });

    if options.all {
        return voicings;
    }

    rank_diverse_voicings(voicings, options.limit)
}

fn rank_diverse_voicings(mut voicings: Vec<Voicing>, limit: usize) -> Vec<Voicing> {
    if limit == 0 {
        return Vec::new();
    }

    let mut selected = Vec::new();
    while selected.len() < limit && !voicings.is_empty() {
        let best_idx = voicings
            .iter()
            .enumerate()
            .min_by_key(|(_, voicing)| effective_voicing_score(voicing, &selected))
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        selected.push(voicings.remove(best_idx));
    }

    selected
}

fn effective_voicing_score(voicing: &Voicing, selected: &[Voicing]) -> u32 {
    let family = voicing_family(&voicing.frets);
    voicing.score
        + u32::try_from(selected_family_count(selected, family)).unwrap_or(0) * 10
        + u32::try_from(selected_position_count(selected, family.position)).unwrap_or(0) * 2
}

fn selected_family_count(selected: &[Voicing], family: VoicingFamily) -> usize {
    selected
        .iter()
        .filter(|voicing| voicing_family(&voicing.frets) == family)
        .count()
}

fn selected_position_count(selected: &[Voicing], position: PositionFamily) -> usize {
    selected
        .iter()
        .filter(|voicing| voicing_family(&voicing.frets).position == position)
        .count()
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct VoicingFamily {
    position: PositionFamily,
    string_band: StringBand,
    density: StringDensity,
    internal_mute: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionFamily {
    OpenPosition,
    OpenHigh(PositionBucket),
    Closed(PositionBucket),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionBucket {
    Low,
    Middle,
    High,
    Upper,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringBand {
    Full,
    Wide,
    Low,
    Middle,
    High,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringDensity {
    Full,
    Four,
    Small,
}

fn voicing_family(frets: &[Option<u8>; STRING_COUNT]) -> VoicingFamily {
    let played = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|_| idx))
        .collect::<Vec<_>>();
    let active_count = played.len();
    let first = played.first().copied().unwrap_or(0);
    let last = played.last().copied().unwrap_or(0);
    let non_open = frets
        .iter()
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    let has_open = frets.iter().flatten().any(|fret| *fret == 0);
    let max_non_open = non_open.iter().copied().max().unwrap_or(0);
    let min_non_open = non_open.iter().copied().min().unwrap_or(0);

    let position = if has_open && max_non_open <= 4 {
        PositionFamily::OpenPosition
    } else if has_open {
        PositionFamily::OpenHigh(position_bucket(min_non_open))
    } else {
        PositionFamily::Closed(position_bucket(min_non_open))
    };

    let string_band = if first == 0 && last == STRING_COUNT - 1 {
        StringBand::Full
    } else if first <= 1 && last >= 4 {
        StringBand::Wide
    } else if last <= 3 {
        StringBand::Low
    } else if first >= 2 {
        StringBand::High
    } else {
        StringBand::Middle
    };

    let density = match active_count {
        5..=STRING_COUNT => StringDensity::Full,
        4 => StringDensity::Four,
        _ => StringDensity::Small,
    };

    VoicingFamily {
        position,
        string_band,
        density,
        internal_mute: internal_mutes(frets) > 0,
    }
}

fn position_bucket(fret: u8) -> PositionBucket {
    match fret {
        0..=4 => PositionBucket::Low,
        5..=8 => PositionBucket::Middle,
        9..=12 => PositionBucket::High,
        _ => PositionBucket::Upper,
    }
}

fn position_cost(non_open: &[u8], has_open: bool) -> u32 {
    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    let multiplier = if has_open { 1 } else { 2 };
    u32::from(*min) * multiplier
}

fn relative_fret_cost(non_open: &[u8]) -> u32 {
    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    non_open
        .iter()
        .map(|fret| u32::from(fret.saturating_sub(*min)))
        .sum()
}

fn fret_span_cost(non_open: &[u8]) -> u32 {
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    let span = u32::from(max.saturating_sub(*min));
    span * span
}

fn active_string_cost(active_count: usize) -> u32 {
    match active_count {
        6 => 0,
        5 => 2,
        4 => 10,
        3 => 24,
        2 => 45,
        1 => 90,
        _ => 120,
    }
}

fn adjacent_fret_jump_cost(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let non_open = frets
        .iter()
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();

    non_open
        .windows(2)
        .map(|window| {
            let jump = window[0].abs_diff(window[1]);
            if jump <= 2 {
                0
            } else {
                let excess = u32::from(jump - 2);
                excess * excess * 4
            }
        })
        .sum()
}

fn duplicate_pitch_cost(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let mut counts = [0u8; 12];
    for (string, fret) in frets.iter().enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = STANDARD_TUNING[string]
            .pitch_class()
            .transpose(i16::from(*fret));
        counts[usize::from(pitch.value())] += 1;
    }

    counts
        .iter()
        .map(|count| count.saturating_sub(2))
        .map(|excess| u32::from(excess) * 8)
        .sum()
}

fn high_open_mix_cost(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let non_open = frets
        .iter()
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    if *max <= 4 {
        return 0;
    }

    let open_indices = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| (*fret == Some(0)).then_some(idx))
        .collect::<Vec<_>>();
    if open_indices.is_empty() {
        return 0;
    }

    let fretted_indices = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.filter(|value| *value > 0).map(|_| idx))
        .collect::<Vec<_>>();
    let first_fretted = fretted_indices.first().copied().unwrap_or(0);
    if open_indices.iter().all(|idx| *idx < first_fretted) {
        return u32::from(max.saturating_sub(8)) * 4;
    }

    let random_high_fret_cost = u32::from(max.saturating_sub(3)) * 8;
    let high_position_open_cost = if *min >= 5 { 24 } else { 0 };
    random_high_fret_cost + high_position_open_cost
}

fn low_open_gap_cost(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let played = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|value| (idx, value)))
        .collect::<Vec<_>>();
    let Some((first_idx, first_fret)) = played.first().copied() else {
        return 0;
    };
    if first_fret == 0 {
        return 0;
    }

    let immediate_open = frets
        .get(first_idx + 1)
        .is_some_and(|fret| *fret == Some(0));
    let fretted_above = frets
        .iter()
        .skip(first_idx + 2)
        .flatten()
        .any(|fret| *fret > 0);
    if immediate_open && fretted_above {
        18
    } else {
        0
    }
}

fn open_position_bonus(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let active_count = frets.iter().flatten().count();
    let has_open = frets.iter().flatten().any(|fret| *fret == 0);
    let max_fret = frets.iter().flatten().copied().max().unwrap_or(0);
    if has_open && max_fret <= 3 && active_count >= 4 {
        match active_count {
            6 => 22,
            5 => 8,
            _ => 4,
        }
    } else {
        0
    }
}

fn open_root_bass_bonus(
    frets: &[Option<u8>; STRING_COUNT],
    expected_bass: Option<PitchClass>,
) -> u32 {
    let Some(expected_bass) = expected_bass else {
        return 0;
    };

    let played = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|value| (idx, value)))
        .collect::<Vec<_>>();
    let Some((first_string, first_fret)) = played.first().copied() else {
        return 0;
    };
    if first_fret != 0 {
        return 0;
    }

    let bass = STANDARD_TUNING[first_string].pitch_class();
    if bass != expected_bass {
        return 0;
    }

    let max_fret = frets.iter().flatten().copied().max().unwrap_or(0);
    if max_fret > 4 {
        return 0;
    }

    match played.len() {
        5..=STRING_COUNT => 18,
        4 => 14,
        _ => 8,
    }
}

fn open_bass_grip_bonus(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let open_indices = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| (*fret == Some(0)).then_some(idx))
        .collect::<Vec<_>>();
    if open_indices.is_empty() {
        return 0;
    }

    let fretted_indices = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.filter(|value| *value > 0).map(|_| idx))
        .collect::<Vec<_>>();
    let (Some(first_fretted), Some(last_fretted)) = (
        fretted_indices.first().copied(),
        fretted_indices.last().copied(),
    ) else {
        return 0;
    };

    if !open_indices.iter().all(|idx| *idx < first_fretted) {
        return 0;
    }
    let contiguous_grip = (first_fretted..=last_fretted).all(|idx| frets[idx].is_some());
    if !contiguous_grip {
        return 0;
    }

    let non_open = frets
        .iter()
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    if *min < 5 {
        return 0;
    }
    if max.saturating_sub(*min) > 4 {
        return 0;
    }

    if fretted_indices.len() >= 5 { 20 } else { 10 }
}

fn closed_shape_bonus(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let active_count = frets.iter().flatten().count();
    let has_open = frets.iter().flatten().any(|fret| *fret == 0);
    if has_open || active_count < 4 || internal_mutes(frets) > 0 {
        return 0;
    }

    let non_open = frets.iter().flatten().copied().collect::<Vec<_>>();
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    let min_count = non_open.iter().filter(|fret| *fret == min).count();
    if min_count < 2 {
        return 0;
    }

    if max.saturating_sub(*min) <= 2 {
        let base = match *min {
            0..=2 => 30,
            3..=5 => 6,
            6..=8 => 12,
            _ => 8,
        };
        if active_count >= 5 { base } else { base / 2 }
    } else {
        0
    }
}

fn internal_mutes(frets: &[Option<u8>; STRING_COUNT]) -> u32 {
    let played = frets
        .iter()
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|_| idx))
        .collect::<Vec<_>>();
    let (Some(first), Some(last)) = (played.first(), played.last()) else {
        return 0;
    };
    let count = (*first..=*last).filter(|idx| frets[*idx].is_none()).count();
    u32::try_from(count).unwrap_or(0)
}

fn can_omit_formula_fifth(missing: PitchSet, formula: &ChordFormula) -> bool {
    if formula.tones.len() < 4 || missing.len() != 1 {
        return false;
    }

    formula.tones.iter().any(|tone| {
        tone.degree == 5 && tone.interval == "5" && missing.contains(PitchClass(tone.pitch_class))
    })
}

pub fn analyze_symbol(input: &str) -> Result<(ChordSymbol, ChordFormula), ChordsmithError> {
    let symbol = ChordSymbol::parse(input)?;
    let formula = symbol.formula();
    reject_redundant_formula(&symbol, &formula)?;
    Ok((symbol, formula))
}

fn reject_redundant_formula(
    symbol: &ChordSymbol,
    formula: &ChordFormula,
) -> Result<(), ChordsmithError> {
    if has_alt_modifiers(&symbol.spec) {
        return Err(ChordsmithError::new(format!(
            "invalid chord symbol '{}': alt is already a complete altered dominant set",
            symbol.name()
        )));
    }
    if has_omitted_alteration(&symbol.spec) {
        return Err(ChordsmithError::new(format!(
            "redundant chord symbol '{}': altered degree is also omitted",
            symbol.name()
        )));
    }
    if has_redundant_alteration(&symbol.spec) {
        return Err(ChordsmithError::new(format!(
            "redundant chord symbol '{}': alteration restates an existing interval",
            symbol.name()
        )));
    }
    if formula.has_duplicate_pitch_classes() {
        return Err(ChordsmithError::new(format!(
            "redundant chord symbol '{}': two or more intervals resolve to the same pitch class",
            symbol.name()
        )));
    }
    Ok(())
}

fn has_alt_modifiers(spec: &ChordSpec) -> bool {
    spec.alt
        && (spec.extension.is_some()
            || spec.sixth
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
}

fn has_omitted_alteration(spec: &ChordSpec) -> bool {
    spec.alterations
        .iter()
        .any(|alteration| spec.omissions.contains(&alteration.degree))
}

fn has_redundant_alteration(spec: &ChordSpec) -> bool {
    let base = normalize_raw_tones(base_raw_tones(spec));
    spec.alterations.iter().any(|alteration| {
        let semitones = natural_semitones(alteration.degree) + i16::from(alteration.accidental);
        base.iter()
            .any(|tone| tone.degree == alteration.degree && tone.semitones == semitones)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

    use super::*;

    fn primary(input: &str) -> String {
        identify(input)
            .unwrap()
            .primary
            .expect("primary analysis")
            .symbol
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
    fn spells_flat_seventh_bass_from_chord_context() {
        let result = identify("x12010").unwrap();
        assert_eq!(result.primary.expect("primary analysis").symbol, "C7/Bb");
        let notes = result
            .notes
            .iter()
            .map(|note| note.note.as_str())
            .collect::<Vec<_>>();
        assert_eq!(notes, ["Bb", "E", "G", "C", "E"]);
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
            .map(|note| note.note.as_str())
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
            .map(|tone| tone.note.as_str())
            .collect::<Vec<_>>();
        assert_eq!(notes, ["C", "E", "G", "A", "D"]);
    }

    #[test]
    fn parses_major_seventh_as_major_not_dominant() {
        let (symbol, formula) = analyze_symbol("Cmaj7").unwrap();
        assert_eq!(symbol.name(), "Cmaj7");
        let notes = formula
            .tones
            .iter()
            .map(|tone| tone.note.as_str())
            .collect::<Vec<_>>();
        assert_eq!(notes, ["C", "E", "G", "B"]);

        let (symbol, formula) = analyze_symbol("CM9").unwrap();
        assert_eq!(symbol.name(), "Cmaj9");
        let notes = formula
            .tones
            .iter()
            .map(|tone| tone.note.as_str())
            .collect::<Vec<_>>();
        assert_eq!(notes, ["C", "E", "G", "B", "D"]);
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
            .map(|tone| tone.interval.as_str())
            .collect::<Vec<_>>();
        assert_eq!(intervals, ["1", "b3", "b5", "bb7"]);

        let (symbol, _) = analyze_symbol("Cø7").unwrap();
        assert_eq!(symbol.name(), "Cm7b5");
    }

    #[test]
    fn parses_alt_as_a_concrete_altered_dominant_set() {
        let (symbol, formula) = analyze_symbol("C7alt").unwrap();
        assert_eq!(symbol.name(), "Calt");
        let intervals = formula
            .tones
            .iter()
            .map(|tone| tone.interval.as_str())
            .collect::<Vec<_>>();
        assert_eq!(intervals, ["1", "3", "b7", "b9", "#9", "b13"]);

        let error = analyze_symbol("Calt#9").expect_err("alt modifiers should fail");
        assert!(error.to_string().contains("alt is already"), "{error}");
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
            .map(|tone| tone.interval.as_str())
            .collect::<Vec<_>>();
        assert_eq!(intervals, ["1", "3", "5", "b7", "b9", "#9"]);

        let (symbol, formula) = analyze_symbol("C13#9b9").unwrap();
        assert_eq!(symbol.name(), "C13b9#9");
        let intervals = formula
            .tones
            .iter()
            .map(|tone| tone.interval.as_str())
            .collect::<Vec<_>>();
        assert_eq!(intervals, ["1", "3", "5", "b7", "b9", "#9", "13"]);
    }

    #[test]
    fn generates_voicings_for_altered_thirteenths() {
        let shapes = voicings(
            "C13b9",
            VoicingOptions {
                max_fret: 12,
                max_span: 4,
                limit: 5,
                all: false,
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
    fn ranks_common_c_shapes_above_weird_valid_shapes() {
        let shapes = voicings(
            "C",
            VoicingOptions {
                max_fret: 12,
                max_span: 4,
                limit: 15,
                all: false,
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
                max_fret: 12,
                max_span: 4,
                limit: 15,
                all: false,
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
    fn includes_common_omitted_fifth_dominant_voicings() {
        let shapes = voicings("C7", VoicingOptions::default()).unwrap();
        let compact = shapes
            .iter()
            .map(|shape| shape.compact.as_str())
            .collect::<Vec<_>>();

        assert_eq!(compact.first().copied(), Some("x32310"));
        assert!(compact.contains(&"x32313"));
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
    fn infers_rootless_and_no_third_omitted_analyses() {
        let rootless = identify("xx2000").unwrap();
        let aliases = rootless
            .aliases
            .iter()
            .map(|analysis| analysis.symbol.as_str())
            .collect::<Vec<_>>();
        assert!(aliases.contains(&"Cmaj7(no1)/E"));

        assert_eq!(primary("x353xx"), "C7(no3)");
    }

    #[test]
    fn classifies_theoretical_aliases_away_from_default_display() {
        let result = identify("x12010").unwrap();
        let dim_alias = result
            .aliases
            .iter()
            .find(|analysis| analysis.symbol == "Bbdim9(no3)")
            .expect("theoretical diminished alias");
        assert_eq!(dim_alias.class, AnalysisClass::TheoreticalAlias);
    }

    #[test]
    fn all_voicings_returns_more_than_the_default_limit() {
        let limited = voicings("C", VoicingOptions::default()).unwrap();
        let all = voicings(
            "C",
            VoicingOptions {
                all: true,
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
    fn rejects_frets_outside_standard_guitar_range() {
        let error = identify("25-x-x-x-x-x").expect_err("fret above 24 should fail");
        assert!(
            error.to_string().contains("standard guitar range"),
            "{error}"
        );

        let error = voicings(
            "C",
            VoicingOptions {
                max_fret: 25,
                ..VoicingOptions::default()
            },
        )
        .expect_err("max fret above 24 should fail");
        assert!(
            error.to_string().contains("standard guitar range"),
            "{error}"
        );

        let error = voicings(
            "C",
            VoicingOptions {
                max_span: 25,
                ..VoicingOptions::default()
            },
        )
        .expect_err("max span above 24 should fail");
        assert!(
            error.to_string().contains("standard guitar range"),
            "{error}"
        );
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
                    .map(|tone| tone.interval.as_str())
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
}
