use std::array;
use std::cmp::Ordering;
use std::fmt::Write as _;
use std::sync::OnceLock;

use serde::Serialize;

use crate::formula::{
    ChordFormula, ChordTone, has_omitted_alteration, has_redundant_alteration, raw_tones_from_spec,
};
use crate::inline_vec::InlineVec;
use crate::notes::{
    Fingering, GuitarTuning, NoteLetter, NoteName, PitchClass, PitchSet, PlayedFingeringCore,
    PlayedNote, STANDARD_TUNING, play_fingering_core,
};
use crate::parse::{push_unique, validate_descriptor};
use crate::symbol::{
    Alteration, ChordSpec, Extension, MAX_ALTERATIONS, MAX_OMISSIONS, Quality, Seventh,
};
use crate::{ChordsmithError, MAX_NOTE_ACCIDENTALS};

const PITCH_SET_KEY_COUNT: usize = 1 << 12;
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

pub fn identify(input: &str) -> Result<IdentifyResult, ChordsmithError> {
    identify_with_tuning(input, STANDARD_TUNING)
}

pub fn identify_with_tuning(
    input: &str,
    tuning: GuitarTuning,
) -> Result<IdentifyResult, ChordsmithError> {
    let fingering = Fingering::parse_with_string_count(input, tuning.string_count())?;
    identify_fingering_with_tuning_ref(&fingering, &tuning)
}

pub fn identify_fingering(fingering: &Fingering) -> Result<IdentifyResult, ChordsmithError> {
    identify_fingering_with_tuning_ref(fingering, &STANDARD_TUNING)
}

pub fn identify_fingering_with_tuning(
    fingering: &Fingering,
    tuning: GuitarTuning,
) -> Result<IdentifyResult, ChordsmithError> {
    identify_fingering_with_tuning_ref(fingering, &tuning)
}

fn identify_fingering_with_tuning_ref(
    fingering: &Fingering,
    tuning: &GuitarTuning,
) -> Result<IdentifyResult, ChordsmithError> {
    if fingering.string_count() != tuning.string_count() {
        return Err(ChordsmithError::new(format!(
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
            if let Some(omissions) = inferred_omission_degrees(missing, &candidate.formula) {
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
        if let Some(omissions) = inferred_omission_degrees(missing, &candidate.formula) {
            candidates.consider(AnalysisCandidate::new(
                candidate, bass, omissions, true, true, true,
            ));
        }
    });

    candidates.sort();
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateRecord {
    root: NoteName,
    spec: ChordSpec,
    formula: ChordFormula,
    formula_set: PitchSet,
}

struct CandidateStore {
    records: Vec<CandidateRecord>,
    exact: [Vec<usize>; PITCH_SET_KEY_COUNT],
    superset_offsets: [usize; PITCH_SET_KEY_COUNT + 1],
    superset_indices: Vec<usize>,
}

impl CandidateStore {
    fn exact_matches(&self, set: PitchSet) -> impl Iterator<Item = &CandidateRecord> {
        self.exact[usize::from(set.bits)]
            .iter()
            .map(|idx| &self.records[*idx])
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
            handle(&self.records[*idx]);
        }
    }
}

fn candidate_store() -> &'static CandidateStore {
    static CANDIDATE_STORE: OnceLock<CandidateStore> = OnceLock::new();
    CANDIDATE_STORE.get_or_init(build_candidate_store)
}

fn build_candidate_store() -> CandidateStore {
    let records = build_candidate_records();
    let mut exact_counts = [0usize; PITCH_SET_KEY_COUNT];
    let mut superset_counts = [0usize; PITCH_SET_KEY_COUNT];

    for record in &records {
        exact_counts[usize::from(record.formula_set.bits)] += 1;
        for_each_subset(record.formula_set, |subset| {
            superset_counts[usize::from(subset.bits)] += 1;
        });
    }

    let mut exact = array::from_fn(|idx| Vec::with_capacity(exact_counts[idx]));
    let superset_offsets = prefix_offsets(&superset_counts);
    let mut superset_indices = vec![0usize; superset_offsets[PITCH_SET_KEY_COUNT]];
    let mut superset_write_offsets = superset_offsets;

    for (idx, record) in records.iter().enumerate() {
        exact[usize::from(record.formula_set.bits)].push(idx);
        for_each_subset(record.formula_set, |subset| {
            let key = usize::from(subset.bits);
            let write_idx = superset_write_offsets[key];
            superset_indices[write_idx] = idx;
            superset_write_offsets[key] += 1;
        });
    }

    CandidateStore {
        records,
        exact,
        superset_offsets,
        superset_indices,
    }
}

fn prefix_offsets(counts: &[usize; PITCH_SET_KEY_COUNT]) -> [usize; PITCH_SET_KEY_COUNT + 1] {
    let mut offsets = [0usize; PITCH_SET_KEY_COUNT + 1];
    let mut idx = 0;
    while idx < PITCH_SET_KEY_COUNT {
        offsets[idx + 1] = offsets[idx] + counts[idx];
        idx += 1;
    }
    offsets
}

fn for_each_subset(formula_set: PitchSet, mut handle: impl FnMut(PitchSet)) {
    let mut subset = formula_set.bits;
    loop {
        handle(PitchSet { bits: subset });
        if subset == 0 {
            break;
        }
        subset = subset.wrapping_sub(1) & formula_set.bits;
    }
}

fn build_candidate_records() -> Vec<CandidateRecord> {
    let mut records = Vec::new();
    for spec in candidate_specs() {
        let raw_tones = raw_tones_from_spec(spec);
        for root in candidate_root_spellings() {
            let formula = ChordFormula::from_raw_parts(*root, &raw_tones);
            if formula.has_duplicate_pitch_classes() {
                continue;
            }
            if formula.has_spelling_outside_double_accidentals() {
                continue;
            }
            records.push(CandidateRecord {
                root: *root,
                spec: spec.clone(),
                formula_set: formula.pitch_set(),
                formula,
            });
        }
    }
    records
}

fn candidate_root_spellings() -> &'static [NoteName] {
    static CANDIDATE_ROOTS: OnceLock<Vec<NoteName>> = OnceLock::new();
    CANDIDATE_ROOTS
        .get_or_init(build_candidate_root_spellings)
        .as_slice()
}

fn build_candidate_root_spellings() -> Vec<NoteName> {
    let mut roots = Vec::new();
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

    if let Some(edge) = edge_spelling_for_pitch(pitch) {
        push_unique_note(notes, edge);
    }
}

fn edge_spelling_for_pitch(pitch: PitchClass) -> Option<NoteName> {
    match pitch.value() {
        0 => Some(NoteName::const_new(NoteLetter::B, 1)),
        4 => Some(NoteName::const_new(NoteLetter::F, -1)),
        5 => Some(NoteName::const_new(NoteLetter::E, 1)),
        11 => Some(NoteName::const_new(NoteLetter::C, -1)),
        _ => None,
    }
}

fn push_unique_note(notes: &mut Vec<NoteName>, note: NoteName) {
    if !notes.contains(&note) {
        notes.push(note);
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

fn candidate_specs() -> &'static [ChordSpec] {
    static CANDIDATE_SPECS: OnceLock<Vec<ChordSpec>> = OnceLock::new();
    CANDIDATE_SPECS
        .get_or_init(build_candidate_specs)
        .as_slice()
}

fn build_candidate_specs() -> Vec<ChordSpec> {
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
            seventh_spec.seventh = *seventh;
            specs.push(seventh_spec.clone());

            for extension in [Extension::Ninth, Extension::Eleventh, Extension::Thirteenth] {
                let mut extended = seventh_spec.clone();
                extended.extension = Some(extension);
                specs.push(extended.clone());
                for altered in alteration_sets(Some(extension)) {
                    let mut spec = extended.clone();
                    spec.alterations = altered;
                    specs.push(spec);
                }
            }

            for altered in alteration_sets(None) {
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
        adds: InlineVec::default(),
        alterations: inline_alterations(&[Alteration {
            degree: 5,
            accidental: -1,
        }]),
        omissions: InlineVec::default(),
    };
    specs.push(half_dim.clone());
    half_dim.extension = Some(Extension::Ninth);
    specs.push(half_dim);

    specs.retain(|spec| {
        validate_descriptor(spec).is_ok()
            && !has_redundant_alteration(spec)
            && !has_omitted_alteration(spec)
    });
    specs.sort_unstable_by_key(ChordSpec::suffix);
    specs.dedup_by(|left, right| left == right);
    specs
}

fn allowed_sevenths(quality: Quality) -> &'static [Seventh] {
    match quality {
        Quality::Diminished => &[Seventh::Diminished],
        Quality::Power => &[],
        _ => &[Seventh::Minor, Seventh::Major],
    }
}

fn alteration_sets(extension: Option<Extension>) -> Vec<InlineVec<Alteration, MAX_ALTERATIONS>> {
    let blocked_degree = extension.map(crate::parse::extension_degree);
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
        let mut option = InlineVec::default();
        let mut blocked = false;
        for (idx, alteration) in alterations.iter().enumerate() {
            if mask & (1usize << idx) != 0 {
                if blocked_degree == Some(alteration.degree) {
                    blocked = true;
                    break;
                }
                let _ = option.push(*alteration);
            }
        }
        if !blocked {
            options.push(option);
        }
    }
    options
}

fn inline_alterations(values: &[Alteration]) -> InlineVec<Alteration, MAX_ALTERATIONS> {
    let mut out = InlineVec::default();
    for value in values {
        let _ = out.push(*value);
    }
    out
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuiltAnalysis {
    analysis: ChordAnalysis,
    spellings: [Option<NoteName>; 12],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct AnalysisCandidate<'a> {
    root: NoteName,
    spec: &'a ChordSpec,
    formula: &'a ChordFormula,
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
        let formula = &record.formula;
        let mut score = analysis_score(root, spec, formula, bass, omitted, show_chord_tone_bass);
        for omission in &omissions {
            score += omission_score(*omission, formula);
        }
        score = score.saturating_sub(rooted_extension_omitted_fifth_bonus(
            formula, omitted, &omissions, slash_bass,
        ));
        score = score.saturating_sub(rooted_seventh_omitted_third_bonus(
            root, bass, formula, omitted, &omissions,
        ));
        score = score.saturating_sub(contextual_omission_adjustment(formula, &omissions));

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
            formula,
            bass,
            omissions,
            omitted,
            show_chord_tone_bass,
            slash_bass,
            score,
        }
    }

    fn materialize(self) -> BuiltAnalysis {
        build_analysis(self)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct AnalysisCandidateSet<'a> {
    items: [Option<AnalysisCandidate<'a>>; MAX_IDENTIFY_ANALYSES],
    len: usize,
}

impl Default for AnalysisCandidateSet<'_> {
    fn default() -> Self {
        Self {
            items: [None; MAX_IDENTIFY_ANALYSES],
            len: 0,
        }
    }
}

impl<'a> AnalysisCandidateSet<'a> {
    fn consider(&mut self, candidate: AnalysisCandidate<'a>) {
        for existing in self.items.iter_mut().take(self.len).flatten() {
            if same_analysis_symbol(existing, &candidate) {
                if compare_analysis_candidates(&candidate, existing).is_lt() {
                    *existing = candidate;
                }
                return;
            }
        }

        if self.len < MAX_IDENTIFY_ANALYSES {
            self.items[self.len] = Some(candidate);
            self.len += 1;
            return;
        }

        let Some(worst_idx) = self.worst_index() else {
            return;
        };
        if self.items[worst_idx]
            .is_some_and(|worst| compare_analysis_candidates(&candidate, &worst).is_lt())
        {
            self.items[worst_idx] = Some(candidate);
        }
    }

    fn worst_index(&self) -> Option<usize> {
        let mut worst: Option<usize> = None;
        for (idx, candidate) in self.items.iter().copied().take(self.len).enumerate() {
            let Some(candidate) = candidate else {
                continue;
            };
            if worst.is_none_or(|worst_idx| {
                self.items[worst_idx].is_some_and(|worst_candidate| {
                    compare_analysis_candidates(&candidate, &worst_candidate).is_gt()
                })
            }) {
                worst = Some(idx);
            }
        }
        worst
    }

    fn sort(&mut self) {
        for idx in 1..self.len {
            let Some(candidate) = self.items[idx] else {
                continue;
            };
            let mut pos = idx;
            while pos > 0 {
                let Some(previous) = self.items[pos - 1] else {
                    break;
                };
                if !compare_analysis_candidates(&candidate, &previous).is_lt() {
                    break;
                }
                self.items[pos] = Some(previous);
                pos -= 1;
            }
            self.items[pos] = Some(candidate);
        }
    }

    fn iter(&self) -> impl Iterator<Item = AnalysisCandidate<'a>> + '_ {
        self.items.iter().copied().take(self.len).flatten()
    }

    const fn len(self) -> usize {
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

fn build_analysis(candidate: AnalysisCandidate<'_>) -> BuiltAnalysis {
    let bass_name = candidate
        .formula
        .tone_for_pitch(candidate.bass)
        .map(|tone| tone.note)
        .unwrap_or_else(|| slash_bass_name(candidate.root, candidate.bass));
    let mut spellings = [None::<NoteName>; 12];
    for tone in &candidate.formula.tones {
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
            notes: candidate
                .formula
                .tones
                .iter()
                .map(|tone| tone.note.to_string())
                .collect(),
            intervals: candidate
                .formula
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

fn omission_score(omission: InferredOmission, formula: &ChordFormula) -> u32 {
    match omission {
        InferredOmission::Root => {
            if formula_supports_rootless_upper_alias(formula) {
                100
            } else {
                500
            }
        }
        InferredOmission::Third => {
            if formula.tones.len() == 4 && formula_has_intervals(formula, &["b7"]) {
                420
            } else {
                500
            }
        }
        InferredOmission::Fifth => {
            if formula_has_upper_structure(formula) {
                40
            } else {
                80
            }
        }
    }
}

fn rooted_extension_omitted_fifth_bonus(
    formula: &ChordFormula,
    omitted: bool,
    omissions: &[InferredOmission],
    slash_bass: bool,
) -> u32 {
    if !slash_bass
        && omitted
        && omissions == [InferredOmission::Fifth]
        && formula.tones.iter().any(|tone| tone.degree == 3)
        && formula.tones.iter().any(|tone| tone.degree == 7)
        && formula
            .tones
            .iter()
            .any(|tone| matches!(tone.degree, 9 | 13))
        && !formula_has_altered_fifth(formula)
    {
        160
    } else {
        0
    }
}

fn rooted_seventh_omitted_third_bonus(
    root: NoteName,
    bass: PitchClass,
    formula: &ChordFormula,
    omitted: bool,
    omissions: &[InferredOmission],
) -> u32 {
    if omitted
        && bass == root.pitch_class()
        && omissions == [InferredOmission::Third]
        && formula_has_seventh(formula)
        && !formula_has_altered_fifth(formula)
    {
        180
    } else {
        0
    }
}

fn analysis_score(
    root: NoteName,
    spec: &ChordSpec,
    formula: &ChordFormula,
    bass: PitchClass,
    omitted: bool,
    penalize_chord_tone_bass: bool,
) -> u32 {
    let mut score = 0u32;
    if bass != root.pitch_class() && penalize_chord_tone_bass {
        score += if omitted {
            400
        } else {
            chord_tone_bass_penalty(formula, bass).unwrap_or(400)
        };
    }
    score += u32::try_from(spec_suffix_len(spec)).unwrap_or(100);
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

fn contextual_omission_adjustment(formula: &ChordFormula, omissions: &[InferredOmission]) -> u32 {
    if is_contextual_altered_upper_alias(formula, omissions) {
        650
    } else if is_contextual_rootless_half_diminished_alias(formula, omissions) {
        500
    } else if is_contextual_rootless_diminished_seventh_alias(formula, omissions) {
        420
    } else if is_contextual_rootless_upper_alias(formula, omissions) {
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
    formula: &ChordFormula,
    omissions: &[InferredOmission],
) -> bool {
    omissions.contains(&InferredOmission::Root)
        && omissions
            .iter()
            .all(|omission| matches!(*omission, InferredOmission::Root | InferredOmission::Fifth))
        && formula.tones.len().saturating_sub(omissions.len()) >= 4
        && !formula_has_altered_fifth(formula)
        && formula_supports_rootless_upper_alias(formula)
}

fn is_contextual_altered_upper_alias(
    formula: &ChordFormula,
    omissions: &[InferredOmission],
) -> bool {
    omissions.contains(&InferredOmission::Root)
        && omissions.contains(&InferredOmission::Third)
        && omissions
            .iter()
            .all(|omission| matches!(*omission, InferredOmission::Root | InferredOmission::Third))
        && formula_has_intervals(formula, &["b7", "b9", "#9", "b13"])
}

fn is_contextual_rootless_diminished_seventh_alias(
    formula: &ChordFormula,
    omissions: &[InferredOmission],
) -> bool {
    omissions == [InferredOmission::Root] && formula_has_intervals(formula, &["b3", "b5", "bb7"])
}

fn is_contextual_rootless_half_diminished_alias(
    formula: &ChordFormula,
    omissions: &[InferredOmission],
) -> bool {
    omissions == [InferredOmission::Root]
        && formula.tones.len() == 4
        && formula_has_intervals(formula, &["b3", "b5", "b7"])
}

fn formula_supports_rootless_upper_alias(formula: &ChordFormula) -> bool {
    formula_has_upper_structure(formula)
        && formula.tones.iter().any(|tone| tone.degree == 7)
        && formula
            .tones
            .iter()
            .any(|tone| tone.interval.degree() == 3 && tone.interval.accidental_delta() == 0)
}

fn formula_has_upper_structure(formula: &ChordFormula) -> bool {
    formula
        .tones
        .iter()
        .any(|tone| matches!(tone.degree, 9 | 11 | 13))
}

fn formula_has_seventh(formula: &ChordFormula) -> bool {
    formula.tones.iter().any(|tone| tone.degree == 7)
}

fn formula_has_altered_fifth(formula: &ChordFormula) -> bool {
    formula
        .tones
        .iter()
        .any(|tone| tone.degree == 5 && !tone.interval.is_natural_fifth())
}

fn formula_has_intervals(formula: &ChordFormula, intervals: &[&str]) -> bool {
    intervals.iter().all(|interval| {
        formula
            .tones
            .iter()
            .any(|tone| interval_matches(tone.interval, interval))
    })
}

fn interval_matches(interval: crate::formula::IntervalName, label: &str) -> bool {
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
        _ => return false,
    };
    interval.degree() == degree && interval.accidental_delta() == accidental_delta
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

fn chord_tone_bass_penalty(formula: &ChordFormula, bass: PitchClass) -> Option<u32> {
    let tone = formula.tone_for_pitch(bass)?;
    Some(match tone.degree {
        1 => 0,
        3 => 45,
        5 => 35,
        7 => 70,
        2 | 4 | 6 | 9 | 11 | 13 => 90,
        _ => 120,
    })
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
