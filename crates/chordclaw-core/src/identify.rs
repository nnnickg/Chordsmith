use std::cmp::Ordering;
use std::fmt::Write as _;

use serde::Serialize;

use crate::candidate_record::{CandidateFormulaSummary, CandidateRecord, CandidateToneSummary};
use crate::formula::{ChordFormula, ChordTone, natural_semitones};
use crate::inline_vec::InlineVec;
use crate::notes::{
    Fingering, GuitarTuning, NoteName, PitchClass, PitchSet, PlayedFingeringCore, PlayedNote,
    STANDARD_TUNING, play_fingering_core,
};
use crate::symbol::{Alteration, ChordSpec, Extension, MAX_OMISSIONS, Quality, Seventh};
use crate::{ChordClawError, MAX_NOTE_ACCIDENTALS};

const MAX_IDENTIFY_ANALYSES: usize = 25;

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

pub fn identify(input: &str) -> Result<IdentifyResult, ChordClawError> {
    identify_with_tuning(input, STANDARD_TUNING)
}

pub fn identify_with_tuning(
    input: &str,
    tuning: GuitarTuning,
) -> Result<IdentifyResult, ChordClawError> {
    let fingering = Fingering::parse_with_string_count(input, tuning.string_count())?;
    identify_fingering_with_tuning_ref(&fingering, &tuning)
}

pub fn identify_fingering(fingering: &Fingering) -> Result<IdentifyResult, ChordClawError> {
    identify_fingering_with_tuning_ref(fingering, &STANDARD_TUNING)
}

pub fn identify_fingering_with_tuning(
    fingering: &Fingering,
    tuning: GuitarTuning,
) -> Result<IdentifyResult, ChordClawError> {
    identify_fingering_with_tuning_ref(fingering, &tuning)
}

fn identify_fingering_with_tuning_ref(
    fingering: &Fingering,
    tuning: &GuitarTuning,
) -> Result<IdentifyResult, ChordClawError> {
    if fingering.string_count() != tuning.string_count() {
        return Err(ChordClawError::new(format!(
            "fingering has {} strings but tuning has {}",
            fingering.string_count(),
            tuning.string_count()
        )));
    }

    let played = play_fingering_core(fingering, tuning);
    let core = identify_core(&played, tuning);
    Ok(materialize_identify_result(fingering, &played, core))
}

fn identify_core(
    played: &PlayedFingeringCore,
    tuning: &GuitarTuning,
) -> AnalysisCandidateSet<'static> {
    let Some(bass) = played.bass else {
        return AnalysisCandidateSet::default();
    };
    if played.set.len() < 2 {
        return AnalysisCandidateSet::default();
    }
    let show_chord_tone_bass = tuning.prefers_root_bass();
    let store = candidate_store();
    let mut candidates = AnalysisCandidateSet::default();
    for candidate in store.exact_matches(played.set) {
        candidates.consider(AnalysisCandidate::new(
            candidate,
            bass,
            InlineVec::default(),
            false,
            show_chord_tone_bass,
            false,
        ));
        if !show_chord_tone_bass && bass != candidate.root.pitch_class() {
            candidates.consider(AnalysisCandidate::new(
                candidate,
                bass,
                InlineVec::default(),
                false,
                true,
                false,
            ));
        }
    }

    let played_without_bass = PitchSet {
        bits: played.set.bits & !(1u16 << bass.value()),
    };
    store.for_each_superset_match(played_without_bass, |candidate| {
        if candidate.formula_set.contains(bass) {
            if candidate.formula_set == played.set {
                return;
            }

            let missing = candidate.formula_set.difference(played.set);
            if let Some(omissions) = inferred_omission_degrees_from_summary(
                missing,
                candidate.summary,
                candidate.formula_set,
            ) {
                candidates.consider(AnalysisCandidate::new(
                    candidate,
                    bass,
                    omissions,
                    true,
                    show_chord_tone_bass,
                    false,
                ));
            }
            return;
        }

        let slash_set = candidate.formula_set.with(bass);
        if played.set == slash_set {
            candidates.consider(AnalysisCandidate::new(
                candidate,
                bass,
                InlineVec::default(),
                false,
                true,
                true,
            ));
            return;
        }

        let missing = candidate.formula_set.difference(played.set);
        if let Some(omissions) = inferred_omission_degrees_from_summary(
            missing,
            candidate.summary,
            candidate.formula_set,
        ) {
            candidates.consider(AnalysisCandidate::new(
                candidate, bass, omissions, true, true, true,
            ));
        }
    });

    candidates
}

fn materialize_identify_result(
    fingering: &Fingering,
    played: &PlayedFingeringCore,
    candidates: AnalysisCandidateSet<'_>,
) -> IdentifyResult {
    let mut built = Vec::with_capacity(candidates.len());
    for candidate in candidates.iter() {
        built.push(candidate.materialize());
    }

    let primary_score = built
        .first()
        .map(|analysis| analysis.analysis.score)
        .unwrap_or(0);
    let primary_spellings = built.first().map(|analysis| &analysis.spellings);
    let primary = built.first().cloned().map(|mut analysis| {
        analysis.analysis.class = AnalysisClass::Primary;
        analysis.analysis
    });
    let notes = respell_played_notes(played.notes.as_slice(), primary_spellings);
    let aliases = built
        .into_iter()
        .skip(1)
        .map(|mut analysis| {
            analysis.analysis.class = classify_alias(&analysis.analysis, primary_score);
            analysis.analysis
        })
        .take(24)
        .collect();

    IdentifyResult {
        fingering: fingering.compact(),
        notes,
        primary,
        aliases,
    }
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/identify_candidate_data.rs"));
}

struct CandidateStore {
    records: &'static [CandidateRecord],
    exact_offsets: &'static [usize],
    exact_indices: &'static [u16],
    superset_offsets: &'static [usize],
    superset_indices: &'static [u16],
}

impl CandidateStore {
    fn exact_matches(&self, set: PitchSet) -> impl Iterator<Item = &CandidateRecord> {
        let key = usize::from(set.bits);
        self.exact_indices[self.exact_offsets[key]..self.exact_offsets[key + 1]]
            .iter()
            .map(|idx| &self.records[usize::from(*idx)])
    }

    fn for_each_superset_match<'a>(
        &'a self,
        set: PitchSet,
        mut handle: impl FnMut(&'a CandidateRecord),
    ) {
        let key = usize::from(set.bits);
        for idx in
            &self.superset_indices[self.superset_offsets[key]..self.superset_offsets[key + 1]]
        {
            handle(&self.records[usize::from(*idx)]);
        }
    }
}

static CANDIDATE_STORE: CandidateStore = CandidateStore {
    records: &generated::CANDIDATE_RECORDS,
    exact_offsets: &generated::EXACT_OFFSETS,
    exact_indices: &generated::EXACT_INDICES,
    superset_offsets: &generated::SUPERSET_OFFSETS,
    superset_indices: &generated::SUPERSET_INDICES,
};

fn candidate_store() -> &'static CandidateStore {
    &CANDIDATE_STORE
}

#[cfg(test)]
mod generated_candidate_data_tests {
    use super::generated;
    use crate::candidate_data_builder::{build_candidate_indices, build_candidate_records};

    #[test]
    fn generated_candidate_data_matches_builder() {
        let records = build_candidate_records();
        let indices = build_candidate_indices(&records);

        assert_eq!(generated::CANDIDATE_RECORDS.as_slice(), records.as_slice());
        assert_eq!(
            generated::EXACT_OFFSETS.as_slice(),
            indices.exact_offsets.as_slice()
        );
        assert_eq!(
            generated::EXACT_INDICES.as_slice(),
            indices
                .exact_indices
                .iter()
                .map(|idx| *idx as u16)
                .collect::<Vec<_>>()
                .as_slice()
        );
        assert_eq!(
            generated::SUPERSET_OFFSETS.as_slice(),
            indices.superset_offsets.as_slice()
        );
        assert_eq!(
            generated::SUPERSET_INDICES.as_slice(),
            indices
                .superset_indices
                .iter()
                .map(|idx| *idx as u16)
                .collect::<Vec<_>>()
                .as_slice()
        );
    }
}

pub(crate) fn prefer_flat_pitch(pitch: PitchClass) -> bool {
    matches!(pitch.value(), 1 | 3 | 8 | 10)
}

fn respell_played_notes(
    notes: &[PlayedNote],
    spellings: Option<&[Option<NoteName>; 12]>,
) -> Vec<PlayedNote> {
    let Some(spellings) = spellings else {
        return notes.to_vec();
    };

    notes
        .iter()
        .map(|played| {
            let mut respelled = *played;
            if let Some(note) = spellings[usize::from(played.pitch_class)]
                && note.accidental.unsigned_abs() <= 1
            {
                respelled.note = note;
            }
            respelled
        })
        .collect()
}

pub(crate) fn can_omit(missing: PitchSet, formula: &ChordFormula) -> bool {
    inferred_omission_degrees(missing, formula).is_some()
}

pub(crate) fn inferred_omission_degrees(
    missing: PitchSet,
    formula: &ChordFormula,
) -> Option<InlineVec<InferredOmission, MAX_OMISSIONS>> {
    if missing.is_empty() {
        return Some(InlineVec::default());
    }

    let mut omissions = InlineVec::default();
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
        push_unique_omission(&mut omissions, InferredOmission::from_degree(tone.degree)?);
    }

    if omissions.is_empty() {
        return None;
    }

    if formula.pitch_set().difference(missing).len() < 2 {
        return None;
    }

    Some(omissions)
}

fn inferred_omission_degrees_from_summary(
    missing: PitchSet,
    summary: CandidateFormulaSummary,
    formula_set: PitchSet,
) -> Option<InlineVec<InferredOmission, MAX_OMISSIONS>> {
    if missing.is_empty() {
        return Some(InlineVec::default());
    }

    let mut omissions = InlineVec::default();
    for pitch in missing.iter() {
        let tone = summary.tone_for_pitch(pitch)?;
        if !can_infer_omission_from_summary(tone, summary) {
            return None;
        }
        if summary.has_degree_not_in(tone.degree, missing) {
            return None;
        }
        push_unique_omission(&mut omissions, InferredOmission::from_degree(tone.degree)?);
    }

    if omissions.is_empty() {
        return None;
    }

    if formula_set.difference(missing).len() < 2 {
        return None;
    }

    Some(omissions)
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum InferredOmission {
    Root,
    Third,
    #[default]
    Fifth,
}

impl InferredOmission {
    const fn from_degree(degree: u8) -> Option<Self> {
        match degree {
            1 => Some(Self::Root),
            3 => Some(Self::Third),
            5 => Some(Self::Fifth),
            _ => None,
        }
    }

    pub(crate) const fn degree(self) -> u8 {
        match self {
            Self::Root => 1,
            Self::Third => 3,
            Self::Fifth => 5,
        }
    }
}

fn push_unique_omission(
    values: &mut InlineVec<InferredOmission, MAX_OMISSIONS>,
    value: InferredOmission,
) {
    if !values.contains(&value) {
        let _ = values.push(value);
        values.sort_unstable();
    }
}

pub(crate) fn can_infer_omission(tone: &ChordTone, formula: &ChordFormula) -> bool {
    match tone.degree {
        1 => formula.tones.len() >= 4,
        3 => formula.tones.len() >= 4 && formula_has_seventh(formula),
        5 => tone.interval.is_natural_fifth(),
        _ => false,
    }
}

fn can_infer_omission_from_summary(
    tone: CandidateToneSummary,
    summary: CandidateFormulaSummary,
) -> bool {
    match tone.degree {
        1 => summary.tone_count >= 4,
        3 => summary.tone_count >= 4 && summary_has_seventh(summary),
        5 => tone.is_natural_fifth(),
        _ => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuiltAnalysis {
    analysis: ChordAnalysis,
    spellings: [Option<NoteName>; 12],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AnalysisCandidate<'a> {
    root: NoteName,
    spec: &'a ChordSpec,
    summary: CandidateFormulaSummary,
    bass: PitchClass,
    omissions: InlineVec<InferredOmission, MAX_OMISSIONS>,
    omitted: bool,
    show_chord_tone_bass: bool,
    slash_bass: bool,
    score: u32,
}

impl<'a> AnalysisCandidate<'a> {
    fn new(
        record: &'a CandidateRecord,
        bass: PitchClass,
        omissions: InlineVec<InferredOmission, MAX_OMISSIONS>,
        omitted: bool,
        show_chord_tone_bass: bool,
        slash_bass: bool,
    ) -> Self {
        let root = record.root;
        let spec = &record.spec;
        let summary = record.summary;
        let mut score = analysis_score(root, spec, summary, bass, omitted, show_chord_tone_bass);
        for omission in &omissions {
            score += omission_score(*omission, summary);
        }
        score = score.saturating_sub(rooted_extension_omitted_fifth_bonus(
            summary, omitted, &omissions, slash_bass,
        ));
        score = score.saturating_sub(rooted_seventh_omitted_third_bonus(
            root, bass, summary, omitted, &omissions,
        ));
        score = score.saturating_sub(contextual_omission_adjustment(summary, &omissions));

        if slash_bass {
            let interval = slash_bass_interval(root, bass);
            let only_fifth_omitted = omissions.as_slice() == [InferredOmission::Fifth];
            score = match (interval, omitted, only_fifth_omitted) {
                (_, false, _) => score
                    .saturating_sub(exact_slash_bass_offset(spec))
                    .saturating_add(exact_slash_bass_penalty(interval)),
                (2, true, true) => score.saturating_sub(520),
                (1 | 3 | 6 | 8, true, true) => score.saturating_sub(400),
                (5, true, _) => score.saturating_add(220),
                _ => score.saturating_add(260),
            };
        }

        Self {
            root,
            spec,
            summary,
            bass,
            omissions,
            omitted,
            show_chord_tone_bass,
            slash_bass,
            score,
        }
    }

    fn materialize(&self) -> BuiltAnalysis {
        build_analysis(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AnalysisCandidateSet<'a> {
    items: [Option<AnalysisCandidate<'a>>; MAX_IDENTIFY_ANALYSES],
    len: usize,
}

impl Default for AnalysisCandidateSet<'_> {
    fn default() -> Self {
        Self {
            items: std::array::from_fn(|_| None),
            len: 0,
        }
    }
}

impl<'a> AnalysisCandidateSet<'a> {
    fn consider(&mut self, candidate: AnalysisCandidate<'a>) {
        for idx in 0..self.len {
            if let Some(existing) = self.items[idx].as_ref()
                && same_analysis_symbol(existing, &candidate)
            {
                if compare_analysis_candidates(&candidate, existing).is_lt() {
                    self.remove(idx);
                    self.insert_sorted(candidate);
                }
                return;
            }
        }

        if self.len == MAX_IDENTIFY_ANALYSES {
            let Some(worst) = self.items[self.len - 1].as_ref() else {
                return;
            };
            if !compare_analysis_candidates(&candidate, worst).is_lt() {
                return;
            }
            self.len -= 1;
            self.items[self.len] = None;
        }

        self.insert_sorted(candidate);
    }

    fn insert_sorted(&mut self, candidate: AnalysisCandidate<'a>) {
        debug_assert!(self.len < MAX_IDENTIFY_ANALYSES);
        let pos = self.insertion_index(&candidate);
        for idx in (pos..self.len).rev() {
            self.items[idx + 1] = self.items[idx].take();
        }
        self.items[pos] = Some(candidate);
        self.len += 1;
    }

    fn insertion_index(&self, candidate: &AnalysisCandidate<'_>) -> usize {
        let mut pos = 0;
        while pos < self.len {
            let Some(existing) = self.items[pos].as_ref() else {
                break;
            };
            if compare_analysis_candidates(candidate, existing).is_lt() {
                break;
            }
            pos += 1;
        }
        pos
    }

    fn remove(&mut self, pos: usize) {
        debug_assert!(pos < self.len);
        for idx in pos..self.len - 1 {
            self.items[idx] = self.items[idx + 1].take();
        }
        self.len -= 1;
        self.items[self.len] = None;
    }

    fn iter(&self) -> impl Iterator<Item = &AnalysisCandidate<'a>> + '_ {
        self.items.iter().take(self.len).filter_map(Option::as_ref)
    }

    const fn len(&self) -> usize {
        self.len
    }
}

fn compare_analysis_candidates(
    left: &AnalysisCandidate<'_>,
    right: &AnalysisCandidate<'_>,
) -> Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| compare_symbol_key(left, right))
}

fn compare_symbol_key(left: &AnalysisCandidate<'_>, right: &AnalysisCandidate<'_>) -> Ordering {
    left.root
        .cmp(&right.root)
        .then_with(|| compare_specs(left.spec, right.spec))
        .then(left.bass.value().cmp(&right.bass.value()))
        .then(left.omissions.as_slice().cmp(right.omissions.as_slice()))
        .then(left.omitted.cmp(&right.omitted))
        .then(left.show_chord_tone_bass.cmp(&right.show_chord_tone_bass))
        .then(left.slash_bass.cmp(&right.slash_bass))
}

fn same_analysis_symbol(left: &AnalysisCandidate<'_>, right: &AnalysisCandidate<'_>) -> bool {
    left.root == right.root
        && left.spec == right.spec
        && same_visible_omissions(&left.omissions, &right.omissions)
        && visible_bass(left) == visible_bass(right)
}

fn same_visible_omissions(
    left: &InlineVec<InferredOmission, MAX_OMISSIONS>,
    right: &InlineVec<InferredOmission, MAX_OMISSIONS>,
) -> bool {
    left.iter()
        .filter(|omission| **omission != InferredOmission::Fifth)
        .eq(right
            .iter()
            .filter(|omission| **omission != InferredOmission::Fifth))
}

fn visible_bass(candidate: &AnalysisCandidate<'_>) -> Option<PitchClass> {
    if candidate.bass != candidate.root.pitch_class() && candidate.show_chord_tone_bass {
        Some(candidate.bass)
    } else {
        None
    }
}

fn compare_specs(left: &ChordSpec, right: &ChordSpec) -> Ordering {
    left.quality
        .cmp(&right.quality)
        .then(left.seventh.cmp(&right.seventh))
        .then(left.extension.cmp(&right.extension))
        .then(left.sixth.cmp(&right.sixth))
        .then(left.alt.cmp(&right.alt))
        .then(left.adds.as_slice().cmp(right.adds.as_slice()))
        .then(
            left.alterations
                .as_slice()
                .cmp(right.alterations.as_slice()),
        )
        .then(left.omissions.as_slice().cmp(right.omissions.as_slice()))
}

fn build_analysis(candidate: &AnalysisCandidate<'_>) -> BuiltAnalysis {
    let formula = ChordFormula::from_parts(candidate.root, candidate.spec);
    let bass_name = formula
        .tone_for_pitch(candidate.bass)
        .map(|tone| tone.note)
        .unwrap_or_else(|| slash_bass_name(candidate.root, candidate.bass));
    let mut spellings = [None::<NoteName>; 12];
    for tone in &formula.tones {
        spellings[usize::from(tone.pitch_class)] = Some(tone.note);
    }
    spellings[usize::from(candidate.bass.value())] = Some(bass_name);

    let mut symbol = format!("{}{}", candidate.root, candidate.spec.suffix());
    if !candidate.omissions.is_empty() {
        for omission in candidate
            .omissions
            .iter()
            .filter(|omission| **omission != InferredOmission::Fifth)
        {
            symbol.push_str("no");
            let _ = write!(&mut symbol, "{}", omission.degree());
        }
    }
    if candidate.bass != candidate.root.pitch_class() && candidate.show_chord_tone_bass {
        symbol.push('/');
        let _ = write!(&mut symbol, "{bass_name}");
    }

    BuiltAnalysis {
        analysis: ChordAnalysis {
            symbol,
            root: candidate.root.to_string(),
            bass: bass_name.to_string(),
            notes: formula
                .tones
                .iter()
                .map(|tone| tone.note.to_string())
                .collect(),
            intervals: formula
                .tones
                .iter()
                .map(|tone| tone.interval.to_string())
                .collect(),
            omissions: omission_strings(&candidate.omissions),
            confidence: if candidate.omitted {
                Confidence::Omitted
            } else {
                Confidence::Exact
            },
            class: AnalysisClass::TheoreticalAlias,
            score: candidate.score,
        },
        spellings,
    }
}

fn omission_strings(omissions: &InlineVec<InferredOmission, MAX_OMISSIONS>) -> Vec<String> {
    omissions
        .iter()
        .map(|omission| omission.degree().to_string())
        .collect()
}

fn slash_bass_name(root: NoteName, bass: PitchClass) -> NoteName {
    let steps = match slash_bass_interval(root, bass) {
        0 => 0,
        1 | 2 => 1,
        3 | 4 => 2,
        5 => 3,
        6 | 7 => 4,
        8 | 9 => 5,
        10 | 11 => 6,
        _ => unreachable!("slash interval is always mod 12"),
    };
    let note = NoteName::spell_for_pitch(root.letter.advance(steps), bass);
    if note.accidental.unsigned_abs() <= MAX_NOTE_ACCIDENTALS {
        note
    } else {
        NoteName::simple_for_pitch(bass, prefer_flat_pitch(bass))
    }
}

fn exact_slash_bass_penalty(interval: u8) -> u32 {
    match interval {
        1 | 3 | 6 | 8 => 18,
        9..=11 => 120,
        2 | 5 | 7 => 10,
        _ => 30,
    }
}

fn exact_slash_bass_offset(spec: &ChordSpec) -> u32 {
    if matches!(spec.quality, Quality::Major | Quality::Minor)
        && spec.seventh == Seventh::None
        && spec.extension.is_none()
        && !spec.sixth
        && !spec.alt
        && spec.adds.is_empty()
        && spec.alterations.is_empty()
    {
        400
    } else {
        340
    }
}

fn slash_bass_interval(root: NoteName, bass: PitchClass) -> u8 {
    (bass.value() + 12 - root.pitch_class().value()) % 12
}

fn classify_alias(analysis: &ChordAnalysis, primary_score: u32) -> AnalysisClass {
    if analysis.score > primary_score.saturating_add(180) {
        return AnalysisClass::TheoreticalAlias;
    }

    if is_edge_root_spelling(&analysis.root) {
        return AnalysisClass::TheoreticalAlias;
    }

    if analysis.confidence == Confidence::Omitted {
        return if is_useful_omitted_alias(analysis) {
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

fn is_edge_root_spelling(root: &str) -> bool {
    matches!(root, "B#" | "E#" | "Cb" | "Fb")
}

fn is_stable_alias_interval(interval: &str) -> bool {
    matches!(
        interval,
        "1" | "2"
            | "b3"
            | "3"
            | "4"
            | "b5"
            | "5"
            | "#5"
            | "6"
            | "bb7"
            | "b7"
            | "7"
            | "9"
            | "11"
            | "13"
    )
}

fn omission_score(omission: InferredOmission, summary: CandidateFormulaSummary) -> u32 {
    match omission {
        InferredOmission::Root => {
            if summary_supports_rootless_upper_alias(summary) {
                100
            } else {
                500
            }
        }
        InferredOmission::Third => {
            if summary.tone_count == 4 && summary_has_intervals(summary, &["b7"]) {
                420
            } else {
                500
            }
        }
        InferredOmission::Fifth => {
            if summary.has_upper_structure() {
                40
            } else {
                80
            }
        }
    }
}

fn rooted_extension_omitted_fifth_bonus(
    summary: CandidateFormulaSummary,
    omitted: bool,
    omissions: &[InferredOmission],
    slash_bass: bool,
) -> u32 {
    if !slash_bass
        && omitted
        && omissions == [InferredOmission::Fifth]
        && summary.has_degree(3)
        && summary.has_degree(7)
        && (summary.has_degree(9) || summary.has_degree(13))
        && !summary_has_altered_fifth(summary)
    {
        160
    } else {
        0
    }
}

fn rooted_seventh_omitted_third_bonus(
    root: NoteName,
    bass: PitchClass,
    summary: CandidateFormulaSummary,
    omitted: bool,
    omissions: &[InferredOmission],
) -> u32 {
    if omitted
        && bass == root.pitch_class()
        && omissions == [InferredOmission::Third]
        && summary_has_seventh(summary)
        && !summary_has_altered_fifth(summary)
    {
        180
    } else {
        0
    }
}

fn analysis_score(
    root: NoteName,
    spec: &ChordSpec,
    summary: CandidateFormulaSummary,
    bass: PitchClass,
    omitted: bool,
    penalize_chord_tone_bass: bool,
) -> u32 {
    let mut score = 0u32;
    if bass != root.pitch_class() && penalize_chord_tone_bass {
        score += if omitted {
            400
        } else {
            chord_tone_bass_penalty_from_summary(summary, bass).unwrap_or(400)
        };
    }
    score += u32::try_from(spec_suffix_len(spec)).unwrap_or(100);
    score += u32::from(root.accidental.unsigned_abs()) * 8;
    score += root_spelling_penalty(root);
    score += u32::from(summary.tone_count) * 4;

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
    if !omitted
        && spec.seventh == Seventh::None
        && spec.extension.is_none()
        && !spec.adds.is_empty()
        && !spec.adds.contains(&13)
        && spec.alterations.is_empty()
    {
        score = score.saturating_sub(35);
    }
    if matches!(spec.quality, Quality::Major)
        && spec.seventh == Seventh::None
        && spec.extension.is_none()
    {
        score = score.saturating_sub(10);
    }
    if !omitted
        && bass == root.pitch_class()
        && matches!(spec.quality, Quality::Major)
        && matches!(spec.seventh, Seventh::Minor)
        && (spec.extension.is_some() || !spec.alterations.is_empty() || spec.alt)
    {
        score = score.saturating_sub(40);
    }
    score
}

fn spec_suffix_len(spec: &ChordSpec) -> usize {
    if spec.quality == Quality::Power {
        return 1;
    }
    if spec.alt {
        return 3;
    }
    if is_special_half_dim(spec) {
        return 4;
    }

    let has_numbered_harmony =
        spec.sixth || spec.seventh != Seventh::None || spec.extension.is_some();
    let deferred_sus_len = match spec.quality {
        Quality::Sus2 | Quality::Sus4 => 4,
        _ => 0,
    };

    let mut len = match spec.quality {
        Quality::Major | Quality::Power => 0,
        Quality::Minor => 1,
        Quality::Diminished | Quality::Augmented => 3,
        Quality::Sus2 | Quality::Sus4 if !has_numbered_harmony => deferred_sus_len,
        Quality::Sus2 | Quality::Sus4 => 0,
    };

    if spec.sixth {
        len += 1;
        if spec.adds.contains(&9) {
            len += 2;
        }
    }

    match spec.extension {
        Some(extension) => len += extension_suffix_len(spec, extension),
        None => {
            len += match spec.seventh {
                Seventh::None => 0,
                Seventh::Minor | Seventh::Diminished => 1,
                Seventh::Major if spec.quality == Quality::Minor => 6,
                Seventh::Major => 4,
            };
        }
    }

    if has_numbered_harmony {
        len += deferred_sus_len;
    }

    for add in &spec.adds {
        if spec.sixth && *add == 9 {
            continue;
        }
        len += 3 + decimal_len(*add);
    }

    for alteration in &spec.alterations {
        let alteration_len =
            alteration.accidental.unsigned_abs() as usize + decimal_len(alteration.degree);
        if len == 0 && alteration.accidental != 0 {
            len += alteration_len + 2;
        } else {
            len += alteration_len;
        }
    }

    for omission in &spec.omissions {
        len += 2 + decimal_len(*omission);
    }

    len
}

fn is_special_half_dim(spec: &ChordSpec) -> bool {
    spec.quality == Quality::Minor
        && spec.seventh == Seventh::Minor
        && spec.extension.is_none()
        && !spec.sixth
        && spec.adds.is_empty()
        && spec.omissions.is_empty()
        && spec.alterations.as_slice()
            == [Alteration {
                degree: 5,
                accidental: -1,
            }]
}

fn extension_suffix_len(spec: &ChordSpec, extension: Extension) -> usize {
    let text_len = match extension {
        Extension::Ninth => 1,
        Extension::Eleventh | Extension::Thirteenth => 2,
    };
    if spec.seventh == Seventh::Major && spec.quality == Quality::Minor {
        5 + text_len
    } else if spec.seventh == Seventh::Major {
        3 + text_len
    } else {
        text_len
    }
}

const fn decimal_len(value: u8) -> usize {
    if value >= 100 {
        3
    } else if value >= 10 {
        2
    } else {
        1
    }
}

fn contextual_omission_adjustment(
    summary: CandidateFormulaSummary,
    omissions: &[InferredOmission],
) -> u32 {
    if is_contextual_altered_upper_alias(summary, omissions) {
        650
    } else if is_contextual_rootless_half_diminished_alias(summary, omissions) {
        500
    } else if is_contextual_rootless_diminished_seventh_alias(summary, omissions) {
        420
    } else if is_contextual_rootless_upper_alias(summary, omissions) {
        if omissions.contains(&InferredOmission::Fifth) {
            220
        } else {
            100
        }
    } else {
        0
    }
}

fn is_contextual_rootless_upper_alias(
    summary: CandidateFormulaSummary,
    omissions: &[InferredOmission],
) -> bool {
    omissions.contains(&InferredOmission::Root)
        && omissions
            .iter()
            .all(|omission| matches!(*omission, InferredOmission::Root | InferredOmission::Fifth))
        && usize::from(summary.tone_count).saturating_sub(omissions.len()) >= 4
        && !summary_has_altered_fifth(summary)
        && summary_supports_rootless_upper_alias(summary)
}

fn is_contextual_altered_upper_alias(
    summary: CandidateFormulaSummary,
    omissions: &[InferredOmission],
) -> bool {
    omissions.contains(&InferredOmission::Root)
        && omissions.contains(&InferredOmission::Third)
        && omissions
            .iter()
            .all(|omission| matches!(*omission, InferredOmission::Root | InferredOmission::Third))
        && summary_has_intervals(summary, &["b7", "b9", "#9", "b13"])
}

fn is_contextual_rootless_diminished_seventh_alias(
    summary: CandidateFormulaSummary,
    omissions: &[InferredOmission],
) -> bool {
    omissions == [InferredOmission::Root] && summary_has_intervals(summary, &["b3", "b5", "bb7"])
}

fn is_contextual_rootless_half_diminished_alias(
    summary: CandidateFormulaSummary,
    omissions: &[InferredOmission],
) -> bool {
    omissions == [InferredOmission::Root]
        && summary.tone_count == 4
        && summary_has_intervals(summary, &["b3", "b5", "b7"])
}

fn formula_has_seventh(formula: &ChordFormula) -> bool {
    formula.tones.iter().any(|tone| tone.degree == 7)
}

fn summary_supports_rootless_upper_alias(summary: CandidateFormulaSummary) -> bool {
    summary.has_upper_structure() && summary.has_degree(7) && summary.has_interval(3, 0)
}

fn summary_has_seventh(summary: CandidateFormulaSummary) -> bool {
    summary.has_degree(7)
}

fn summary_has_altered_fifth(summary: CandidateFormulaSummary) -> bool {
    for pitch in 0..12 {
        if summary.degree_by_pitch[pitch] == 5
            && i16::from(summary.semitone_by_pitch[pitch]) != natural_semitones(5)
        {
            return true;
        }
    }
    false
}

fn summary_has_intervals(summary: CandidateFormulaSummary, intervals: &[&str]) -> bool {
    intervals.iter().all(|interval| {
        let Some((degree, accidental_delta)) = interval_label(interval) else {
            return false;
        };
        summary.has_interval(degree, accidental_delta)
    })
}

fn interval_label(label: &str) -> Option<(u8, i16)> {
    let (degree, accidental_delta) = match label {
        "1" => (1, 0),
        "2" => (2, 0),
        "b3" => (3, -1),
        "3" => (3, 0),
        "4" => (4, 0),
        "b5" => (5, -1),
        "5" => (5, 0),
        "#5" => (5, 1),
        "6" => (6, 0),
        "bb7" => (7, -2),
        "b7" => (7, -1),
        "7" => (7, 0),
        "b9" => (9, -1),
        "9" => (9, 0),
        "#9" => (9, 1),
        "11" => (11, 0),
        "#11" => (11, 1),
        "b13" => (13, -1),
        "13" => (13, 0),
        _ => return None,
    };
    Some((degree, accidental_delta))
}

fn is_useful_omitted_alias(analysis: &ChordAnalysis) -> bool {
    if analysis.omissions == ["5"] {
        return true;
    }

    is_useful_rootless_upper_alias(analysis)
        || is_useful_altered_upper_alias(analysis)
        || is_useful_rootless_diminished_seventh_alias(analysis)
        || is_useful_rootless_half_diminished_alias(analysis)
}

fn is_useful_rootless_upper_alias(analysis: &ChordAnalysis) -> bool {
    analysis.omissions.iter().any(|omission| omission == "1")
        && analysis
            .omissions
            .iter()
            .all(|omission| matches!(omission.as_str(), "1" | "5"))
        && (analysis
            .intervals
            .len()
            .saturating_sub(analysis.omissions.len())
            >= 4
            || analysis_has_dominant_ninth_shell(analysis)
            || analysis_has_altered_dominant_upper(analysis))
        && !analysis
            .intervals
            .iter()
            .any(|interval| matches!(interval.as_str(), "b5" | "#5"))
        && analysis
            .intervals
            .iter()
            .any(|interval| matches!(interval.as_str(), "b7" | "7"))
        && analysis.intervals.iter().any(|interval| {
            matches!(
                interval.as_str(),
                "b9" | "9" | "#9" | "11" | "#11" | "b13" | "13"
            )
        })
}

fn analysis_has_altered_dominant_upper(analysis: &ChordAnalysis) -> bool {
    analysis
        .intervals
        .iter()
        .any(|interval| matches!(interval.as_str(), "b7"))
        && analysis
            .intervals
            .iter()
            .any(|interval| matches!(interval.as_str(), "b9" | "#9" | "#11" | "b13"))
}

fn analysis_has_dominant_ninth_shell(analysis: &ChordAnalysis) -> bool {
    ["3", "b7", "9"]
        .iter()
        .all(|interval| analysis.intervals.iter().any(|value| value == interval))
}

fn is_useful_altered_upper_alias(analysis: &ChordAnalysis) -> bool {
    analysis.omissions.iter().any(|omission| omission == "1")
        && analysis.omissions.iter().any(|omission| omission == "3")
        && analysis
            .omissions
            .iter()
            .all(|omission| matches!(omission.as_str(), "1" | "3"))
        && ["b7", "b9", "#9", "b13"]
            .iter()
            .all(|interval| analysis.intervals.iter().any(|value| value == interval))
}

fn is_useful_rootless_diminished_seventh_alias(analysis: &ChordAnalysis) -> bool {
    analysis.omissions == ["1"]
        && ["b3", "b5", "bb7"]
            .iter()
            .all(|interval| analysis.intervals.iter().any(|value| value == interval))
}

fn is_useful_rootless_half_diminished_alias(analysis: &ChordAnalysis) -> bool {
    analysis.omissions == ["1"]
        && analysis.intervals.len() == 4
        && ["b3", "b5", "b7"]
            .iter()
            .all(|interval| analysis.intervals.iter().any(|value| value == interval))
}

fn chord_tone_bass_penalty_from_summary(
    summary: CandidateFormulaSummary,
    bass: PitchClass,
) -> Option<u32> {
    let tone = summary.tone_for_pitch(bass)?;
    Some(chord_tone_bass_penalty_for_degree(tone.degree))
}

fn chord_tone_bass_penalty_for_degree(degree: u8) -> u32 {
    match degree {
        1 => 0,
        3 => 45,
        5 => 35,
        7 => 70,
        2 | 4 | 6 | 9 | 11 | 13 => 90,
        _ => 120,
    }
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
