use std::fmt;

pub const GUITAR_STRING_COUNT: usize = 6;
pub const GUITAR7_STRING_COUNT: usize = 7;
pub const GUITAR8_STRING_COUNT: usize = 8;
pub const UKULELE_STRING_COUNT: usize = 4;
pub const STRING_COUNT: usize = GUITAR_STRING_COUNT;
pub(crate) const MAX_STRING_COUNT: usize = GUITAR8_STRING_COUNT;
pub const DEFAULT_MIN_FRET: u8 = 0;
pub const DEFAULT_MAX_FRET: u8 = 12;
pub const DEFAULT_MAX_SPAN: u8 = 4;
pub const DEFAULT_LIMIT: usize = 15;
pub const MAX_LIMIT: usize = 1_000;
pub const MAX_STANDARD_FRET: u8 = 30;
pub const MAX_ALL_VOICINGS: usize = 25_000;
pub(crate) const MAX_NOTE_ACCIDENTALS: u8 = 2;
pub(crate) const MAX_DIVERSITY_SCORE_WINDOW: u32 = 12;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChordClawErrorKind {
    Data,
    Usage,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChordClawError {
    kind: ChordClawErrorKind,
    message: String,
}

impl ChordClawError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self::with_kind(ChordClawErrorKind::Data, message)
    }

    pub fn with_kind(kind: ChordClawErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> ChordClawErrorKind {
        self.kind
    }
}

impl fmt::Display for ChordClawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChordClawError {}

#[cfg(test)]
mod candidate_data_builder;
mod candidate_record;
mod formula;
mod identify;
mod inline_vec;
mod notes;
mod parse;
mod scoring;
mod symbol;
mod voicing;

pub use formula::{ChordFormula, ChordTone, IntervalName, analyze_symbol};
pub use identify::{
    AnalysisClass, ChordAnalysis, Confidence, IdentifyResult, identify, identify_fingering,
    identify_fingering_with_tuning, identify_with_tuning,
};
pub use notes::{
    Fingering, GuitarTuning, Instrument, NoteLetter, NoteName, PitchClass, PlayedNote,
    STANDARD_7_STRING_TUNING, STANDARD_7_STRING_TUNING_NOTES, STANDARD_8_STRING_TUNING,
    STANDARD_8_STRING_TUNING_NOTES, STANDARD_TUNING, STANDARD_TUNING_NOTES,
    STANDARD_UKULELE_TUNING, STANDARD_UKULELE_TUNING_NOTES, Tuning, UKULELE_TUNING, UkuleleTuning,
};
pub use scoring::VoicingScoreBreakdown;
pub use symbol::{Alteration, ChordSpec, ChordSymbol, Extension, Quality, Seventh};
pub use voicing::{
    Voicing, VoicingMode, VoicingOptions, VoicingScoreContext, voicing_score_breakdown_with_tuning,
    voicings, voicings_with_tuning,
};

#[cfg(test)]
mod tests;
