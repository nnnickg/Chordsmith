use std::cmp::Ordering;

use serde::Serialize;

use crate::ChordClawError;
use crate::formula::{
    ChordFormula, reject_enharmonic_chord_tone_bass, reject_redundant_formula,
    reject_redundant_root_bass,
};
use crate::inline_vec::InlineVec;
use crate::notes::NoteName;
use crate::parse::{normalize_chart_glyphs, parse_descriptor, parse_note_prefix, split_bass};

pub(crate) const MAX_ADDS: usize = 4;
pub(crate) const MAX_ALTERATIONS: usize = 8;
pub(crate) const MAX_OMISSIONS: usize = 4;

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

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct Alteration {
    pub(crate) degree: u8,
    pub(crate) accidental: i8,
}

impl Alteration {
    pub fn new(degree: u8, accidental: i8) -> Result<Self, ChordClawError> {
        if !matches!(degree, 5 | 9 | 11 | 13) {
            return Err(ChordClawError::new(format!(
                "invalid alteration degree '{degree}'"
            )));
        }
        if !is_supported_alteration(degree, accidental) {
            return Err(ChordClawError::new(format!(
                "unsupported alteration '{}{}'",
                alteration_prefix(accidental),
                degree
            )));
        }
        Ok(Self { degree, accidental })
    }

    pub const fn degree(self) -> u8 {
        self.degree
    }

    pub const fn accidental(self) -> i8 {
        self.accidental
    }
}

pub(crate) const fn is_supported_alteration(degree: u8, accidental: i8) -> bool {
    matches!(
        (degree, accidental),
        (5, -1) | (5, 1) | (9, -1) | (9, 1) | (11, 1) | (13, -1)
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordSpec {
    pub(crate) quality: Quality,
    pub(crate) seventh: Seventh,
    pub(crate) extension: Option<Extension>,
    pub(crate) sixth: bool,
    pub(crate) alt: bool,
    pub(crate) adds: InlineVec<u8, MAX_ADDS>,
    pub(crate) alterations: InlineVec<Alteration, MAX_ALTERATIONS>,
    pub(crate) omissions: InlineVec<u8, MAX_OMISSIONS>,
}

impl Default for ChordSpec {
    fn default() -> Self {
        Self {
            quality: Quality::Major,
            seventh: Seventh::None,
            extension: None,
            sixth: false,
            alt: false,
            adds: InlineVec::default(),
            alterations: InlineVec::default(),
            omissions: InlineVec::default(),
        }
    }
}

impl ChordSpec {
    pub const fn quality(&self) -> Quality {
        self.quality
    }

    pub const fn seventh(&self) -> Seventh {
        self.seventh
    }

    pub const fn extension(&self) -> Option<Extension> {
        self.extension
    }

    pub const fn sixth(&self) -> bool {
        self.sixth
    }

    pub const fn alt(&self) -> bool {
        self.alt
    }

    pub fn adds(&self) -> &[u8] {
        &self.adds
    }

    pub fn alterations(&self) -> &[Alteration] {
        &self.alterations
    }

    pub fn omissions(&self) -> &[u8] {
        &self.omissions
    }

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
            && self.omissions.is_empty()
            && self.alterations.as_slice()
                == [Alteration {
                    degree: 5,
                    accidental: -1,
                }];

        if special_half_dim {
            return "m7b5".to_owned();
        }

        let has_numbered_harmony =
            self.sixth || self.seventh != Seventh::None || self.extension.is_some();
        let deferred_sus = match self.quality {
            Quality::Sus2 => Some("sus2"),
            Quality::Sus4 => Some("sus4"),
            _ => None,
        };

        match self.quality {
            Quality::Major => {}
            Quality::Minor => out.push('m'),
            Quality::Diminished => out.push_str("dim"),
            Quality::Augmented => out.push_str("aug"),
            Quality::Sus2 | Quality::Sus4 if !has_numbered_harmony => {
                out.push_str(deferred_sus.unwrap_or(""))
            }
            Quality::Sus2 | Quality::Sus4 => {}
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

        if has_numbered_harmony && let Some(sus) = deferred_sus {
            out.push_str(sus);
        }

        for add in &self.adds {
            if self.sixth && *add == 9 {
                continue;
            }
            out.push_str("add");
            out.push_str(&add.to_string());
        }

        for alteration in &self.alterations {
            let alteration_text = format!(
                "{}{}",
                alteration_prefix(alteration.accidental),
                alteration.degree
            );
            if out.is_empty() && alteration_text.starts_with(['b', '#']) {
                out.push('(');
                out.push_str(&alteration_text);
                out.push(')');
            } else {
                out.push_str(&alteration_text);
            }
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

pub(crate) fn alteration_prefix(accidental: i8) -> String {
    match accidental.cmp(&0) {
        Ordering::Less => "b".repeat(accidental.unsigned_abs() as usize),
        Ordering::Equal => String::new(),
        Ordering::Greater => "#".repeat(accidental.unsigned_abs() as usize),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChordSymbol {
    pub(crate) root: NoteName,
    pub(crate) spec: ChordSpec,
    pub(crate) bass: Option<NoteName>,
}

impl ChordSymbol {
    pub fn parse(input: &str) -> Result<Self, ChordClawError> {
        let normalized = normalize_chart_glyphs(input);
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            return Err(ChordClawError::new("empty chord symbol"));
        }

        let (root, rest) = parse_note_prefix(trimmed)?;
        let (descriptor, bass) = split_bass(rest)?;
        let spec = parse_descriptor(descriptor)?;
        let symbol = Self { root, spec, bass };
        symbol.validate()?;
        Ok(symbol)
    }

    pub fn formula(&self) -> ChordFormula {
        ChordFormula::from_parts(self.root, &self.spec)
    }

    fn validate(&self) -> Result<(), ChordClawError> {
        let formula = self.formula();
        reject_redundant_root_bass(self)?;
        reject_enharmonic_chord_tone_bass(self, &formula)?;
        reject_redundant_formula(self, &formula)
    }

    pub const fn root(&self) -> NoteName {
        self.root
    }

    pub const fn bass(&self) -> Option<NoteName> {
        self.bass
    }

    pub const fn spec(&self) -> &ChordSpec {
        &self.spec
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
