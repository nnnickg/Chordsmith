use crate::notes::{NoteName, PitchSet};
use crate::symbol::ChordSpec;

const EMPTY_TONE_DEGREE: u8 = 0;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct CandidateFormulaSummary {
    pub(crate) tone_count: u8,
    pub(crate) degree_by_pitch: [u8; 12],
    pub(crate) semitone_by_pitch: [i8; 12],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct CandidateToneSummary {
    pub(crate) degree: u8,
    pub(crate) semitones: i16,
}

impl CandidateFormulaSummary {
    pub(crate) fn tone_for_pitch(
        self,
        pitch: crate::notes::PitchClass,
    ) -> Option<CandidateToneSummary> {
        let idx = usize::from(pitch.value());
        let degree = self.degree_by_pitch[idx];
        if degree == EMPTY_TONE_DEGREE {
            None
        } else {
            Some(CandidateToneSummary {
                degree,
                semitones: i16::from(self.semitone_by_pitch[idx]),
            })
        }
    }

    pub(crate) fn has_degree_not_in(self, degree: u8, missing: PitchSet) -> bool {
        for pitch in 0..12 {
            if self.degree_by_pitch[pitch] == degree && missing.bits & (1u16 << pitch) == 0 {
                return true;
            }
        }
        false
    }

    pub(crate) fn has_degree(self, degree: u8) -> bool {
        self.degree_by_pitch.contains(&degree)
    }

    pub(crate) fn has_interval(self, degree: u8, accidental_delta: i16) -> bool {
        for pitch in 0..12 {
            if self.degree_by_pitch[pitch] == degree {
                let semitones = i16::from(self.semitone_by_pitch[pitch]);
                if semitones - crate::formula::natural_semitones(degree) == accidental_delta {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn has_upper_structure(self) -> bool {
        self.has_degree(9) || self.has_degree(11) || self.has_degree(13)
    }
}

impl CandidateToneSummary {
    pub(crate) fn is_natural_fifth(self) -> bool {
        self.degree == 5 && self.semitones == crate::formula::natural_semitones(5)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CandidateRecord {
    pub(crate) root: NoteName,
    pub(crate) spec: ChordSpec,
    pub(crate) summary: CandidateFormulaSummary,
    pub(crate) formula_set: PitchSet,
}
