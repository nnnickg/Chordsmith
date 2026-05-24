use std::cmp::Ordering;
use std::fmt::{self, Write as _};

use serde::{Serialize, Serializer};

use crate::inline_vec::InlineVec;
use crate::notes::{NoteName, PitchClass, PitchSet};
use crate::symbol::{ChordSpec, ChordSymbol, Extension, Quality, Seventh};
use crate::{ChordsmithError, MAX_NOTE_ACCIDENTALS};

pub(crate) const MAX_FORMULA_TONES: usize = 16;

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ChordTone {
    pub degree: u8,
    pub interval: IntervalName,
    pub semitones: i16,
    pub note: NoteName,
    pub pitch_class: u8,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct IntervalName {
    degree: u8,
    semitones: i16,
}

impl IntervalName {
    const fn new(degree: u8, semitones: i16) -> Self {
        Self { degree, semitones }
    }

    pub(crate) fn is_natural_fifth(self) -> bool {
        self.degree == 5 && self.semitones == natural_semitones(5)
    }
}

impl fmt::Display for IntervalName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let natural = natural_semitones(self.degree);
        let delta = self.semitones - natural;
        let ch = match delta.cmp(&0) {
            Ordering::Less => 'b',
            Ordering::Equal => '\0',
            Ordering::Greater => '#',
        };
        if ch != '\0' {
            for _ in 0..delta.unsigned_abs() {
                f.write_char(ch)?;
            }
        }
        write!(f, "{}", self.degree)
    }
}

impl Serialize for IntervalName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordFormula {
    pub(crate) tones: InlineVec<ChordTone, MAX_FORMULA_TONES>,
}

impl ChordFormula {
    pub fn tones(&self) -> &[ChordTone] {
        &self.tones
    }

    pub(crate) fn from_parts(root: NoteName, spec: &ChordSpec) -> Self {
        let mut tones = InlineVec::default();
        for tone in &raw_tones_from_spec(spec) {
            let pitch = root.pitch_class().transpose(tone.semitones);
            let letter = root.letter.advance(degree_letter_steps(tone.degree));
            let note = NoteName::spell_for_pitch(letter, pitch);
            let _ = tones.push(ChordTone {
                degree: tone.degree,
                interval: IntervalName::new(tone.degree, tone.semitones),
                semitones: tone.semitones,
                note,
                pitch_class: pitch.value(),
            });
        }

        Self { tones }
    }

    pub(crate) fn pitch_set(&self) -> PitchSet {
        let mut set = PitchSet::empty();
        for tone in &self.tones {
            set.insert(PitchClass(tone.pitch_class));
        }
        set
    }

    pub(crate) fn has_duplicate_pitch_classes(&self) -> bool {
        self.pitch_set().len() != self.tones.len()
    }

    pub(crate) fn has_spelling_outside_double_accidentals(&self) -> bool {
        self.tones
            .iter()
            .any(|tone| tone.note.accidental.unsigned_abs() > MAX_NOTE_ACCIDENTALS)
    }

    pub(crate) fn tone_for_pitch(&self, pitch: PitchClass) -> Option<&ChordTone> {
        self.tones
            .iter()
            .find(|tone| tone.pitch_class == pitch.value())
    }
}

fn raw_tones_from_spec(spec: &ChordSpec) -> InlineVec<RawTone, MAX_FORMULA_TONES> {
    let mut raw = raw_tones_without_omissions(spec);

    for omission in &spec.omissions {
        raw.retain(|tone| tone.degree != *omission);
    }

    normalize_raw_tones(raw)
}

pub(crate) fn raw_tones_without_omissions(
    spec: &ChordSpec,
) -> InlineVec<RawTone, MAX_FORMULA_TONES> {
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

    normalize_raw_tones(raw)
}

fn base_raw_tones(spec: &ChordSpec) -> InlineVec<RawTone, MAX_FORMULA_TONES> {
    let mut raw = InlineVec::<RawTone, MAX_FORMULA_TONES>::default();
    if spec.alt {
        for tone in [
            RawTone::new(1, 0),
            RawTone::new(3, 4),
            RawTone::new(7, 10),
            RawTone::new(9, 13),
            RawTone::new(9, 15),
            RawTone::new(13, 20),
        ] {
            let _ = raw.push(tone);
        }
        return raw;
    }

    let _ = raw.push(RawTone::new(1, 0));

    match spec.quality {
        Quality::Major => {
            let _ = raw.push(RawTone::new(3, 4));
            let _ = raw.push(RawTone::new(5, 7));
        }
        Quality::Minor => {
            let _ = raw.push(RawTone::new(3, 3));
            let _ = raw.push(RawTone::new(5, 7));
        }
        Quality::Diminished => {
            let _ = raw.push(RawTone::new(3, 3));
            let _ = raw.push(RawTone::new(5, 6));
        }
        Quality::Augmented => {
            let _ = raw.push(RawTone::new(3, 4));
            let _ = raw.push(RawTone::new(5, 8));
        }
        Quality::Sus2 => {
            let _ = raw.push(RawTone::new(2, 2));
            let _ = raw.push(RawTone::new(5, 7));
        }
        Quality::Sus4 => {
            let _ = raw.push(RawTone::new(4, 5));
            let _ = raw.push(RawTone::new(5, 7));
        }
        Quality::Power => {
            let _ = raw.push(RawTone::new(5, 7));
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

fn normalize_raw_tones(
    mut raw: InlineVec<RawTone, MAX_FORMULA_TONES>,
) -> InlineVec<RawTone, MAX_FORMULA_TONES> {
    raw.sort_by_key(|tone| (degree_order(tone.degree), tone.semitones));
    raw.dedup();
    raw
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RawTone {
    pub(crate) degree: u8,
    pub(crate) semitones: i16,
}

impl RawTone {
    const fn new(degree: u8, semitones: i16) -> Self {
        Self { degree, semitones }
    }
}

fn upsert_tone(tones: &mut InlineVec<RawTone, MAX_FORMULA_TONES>, tone: RawTone) {
    tones.retain(|item| item.degree != tone.degree);
    let _ = tones.push(tone);
}

fn push_tone(tones: &mut InlineVec<RawTone, MAX_FORMULA_TONES>, tone: RawTone) {
    if !tones.contains(&tone) {
        let _ = tones.push(tone);
    }
}

pub(crate) fn natural_semitones(degree: u8) -> i16 {
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
        _ => unreachable!("validated chord degree"),
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

pub(crate) fn degree_letter_steps(degree: u8) -> u8 {
    match degree {
        1 => 0,
        2 | 9 => 1,
        3 => 2,
        4 | 11 => 3,
        5 => 4,
        6 | 13 => 5,
        7 => 6,
        _ => unreachable!("validated chord degree"),
    }
}

pub fn analyze_symbol(input: &str) -> Result<(ChordSymbol, ChordFormula), ChordsmithError> {
    let symbol = ChordSymbol::parse(input)?;
    let formula = symbol.formula();
    Ok((symbol, formula))
}

pub(crate) fn reject_redundant_root_bass(symbol: &ChordSymbol) -> Result<(), ChordsmithError> {
    let Some(bass) = symbol.bass else {
        return Ok(());
    };
    if bass != symbol.root {
        return Ok(());
    }

    Err(ChordsmithError::new(format!(
        "invalid slash chord bass '{}': bass repeats root; use '{}{}'",
        bass,
        symbol.root,
        symbol.spec.suffix()
    )))
}

pub(crate) fn reject_enharmonic_chord_tone_bass(
    symbol: &ChordSymbol,
    formula: &ChordFormula,
) -> Result<(), ChordsmithError> {
    let Some(bass) = symbol.bass else {
        return Ok(());
    };
    let Some(tone) = formula.tone_for_pitch(bass.pitch_class()) else {
        return Ok(());
    };
    let bass_text = bass.to_string();
    if tone.note == bass {
        return Ok(());
    }

    Err(ChordsmithError::new(format!(
        "invalid slash chord bass '{bass_text}': use chord-tone spelling '{}'",
        tone.note
    )))
}

pub(crate) fn reject_redundant_formula(
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
    if formula.has_spelling_outside_double_accidentals() {
        return Err(ChordsmithError::new(format!(
            "invalid chord symbol '{}': formula spelling requires accidentals beyond double sharps or flats",
            symbol.name()
        )));
    }
    Ok(())
}

pub(crate) fn has_alt_modifiers(spec: &ChordSpec) -> bool {
    spec.alt
        && (spec.extension.is_some()
            || spec.sixth
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
}

pub(crate) fn has_omitted_alteration(spec: &ChordSpec) -> bool {
    spec.alterations
        .iter()
        .any(|alteration| spec.omissions.contains(&alteration.degree))
}

pub(crate) fn has_redundant_alteration(spec: &ChordSpec) -> bool {
    let base = normalize_raw_tones(base_raw_tones(spec));
    spec.alterations.iter().any(|alteration| {
        let semitones = natural_semitones(alteration.degree) + i16::from(alteration.accidental);
        base.iter()
            .any(|tone| tone.degree == alteration.degree && tone.semitones == semitones)
    })
}
