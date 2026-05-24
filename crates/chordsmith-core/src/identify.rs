use std::array;
use std::fmt::Write as _;
use std::sync::OnceLock;

use serde::Serialize;

use crate::formula::{ChordFormula, ChordTone, has_omitted_alteration, has_redundant_alteration};
use crate::inline_vec::InlineVec;
use crate::notes::{
    Fingering, GuitarTuning, NoteLetter, NoteName, PitchClass, PitchSet, PlayedNote,
    STANDARD_TUNING, play_fingering,
};
use crate::parse::{push_unique, validate_descriptor};
use crate::symbol::{Alteration, ChordSpec, Extension, MAX_ALTERATIONS, Quality, Seventh};
use crate::{ChordsmithError, MAX_NOTE_ACCIDENTALS};

const PITCH_SET_KEY_COUNT: usize = 1 << 12;

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
    identify_fingering_with_tuning(&fingering, tuning)
}

pub fn identify_fingering(fingering: &Fingering) -> Result<IdentifyResult, ChordsmithError> {
    identify_fingering_with_tuning(fingering, STANDARD_TUNING)
}

pub fn identify_fingering_with_tuning(
    fingering: &Fingering,
    tuning: GuitarTuning,
) -> Result<IdentifyResult, ChordsmithError> {
    if fingering.string_count() != tuning.string_count() {
        return Err(ChordsmithError::new(format!(
            "fingering has {} strings but tuning has {}",
            fingering.string_count(),
            tuning.string_count()
        )));
    }

    let played = play_fingering(fingering, tuning);
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

    let show_chord_tone_bass = tuning.prefers_root_bass();
    let store = candidate_store();
    let mut analyses = Vec::new();
    for candidate in store.exact_matches(played.set) {
        analyses.push(build_analysis(
            candidate.root,
            &candidate.spec,
            &candidate.formula,
            bass,
            Vec::new(),
            false,
            show_chord_tone_bass,
        ));
        if !show_chord_tone_bass && bass != candidate.root.pitch_class() {
            analyses.push(build_analysis(
                candidate.root,
                &candidate.spec,
                &candidate.formula,
                bass,
                Vec::new(),
                false,
                true,
            ));
        }
    }

    let played_without_bass = PitchSet {
        bits: played.set.bits & !(1u16 << bass.value()),
    };
    store.for_each_superset_match(played_without_bass, |candidate| {
        if candidate.formula_set.contains(bass) {
            return;
        }

        let slash_set = candidate.formula_set.with(bass);
        if played.set == slash_set {
            analyses.push(build_slash_bass_analysis(
                candidate.root,
                &candidate.spec,
                &candidate.formula,
                bass,
                Vec::new(),
                false,
            ));
            return;
        }

        let missing = candidate.formula_set.difference(played.set);
        if let Some(omissions) = inferred_omissions(missing, &candidate.formula) {
            analyses.push(build_slash_bass_analysis(
                candidate.root,
                &candidate.spec,
                &candidate.formula,
                bass,
                omissions,
                true,
            ));
        }
    });

    store.for_each_superset_match(played.set, |candidate| {
        if candidate.formula_set == played.set {
            return;
        }

        let missing = candidate.formula_set.difference(played.set);
        if let Some(omissions) = inferred_omissions(missing, &candidate.formula) {
            analyses.push(build_analysis(
                candidate.root,
                &candidate.spec,
                &candidate.formula,
                bass,
                omissions,
                true,
                show_chord_tone_bass,
            ));
        }
    });

    analyses.sort_by(|left, right| {
        left.analysis
            .score
            .cmp(&right.analysis.score)
            .then(left.analysis.symbol.cmp(&right.analysis.symbol))
    });
    analyses.dedup_by(|left, right| left.analysis.symbol == right.analysis.symbol);

    let primary_score = analyses
        .first()
        .map(|analysis| analysis.analysis.score)
        .unwrap_or(0);
    let primary_spellings = analyses.first().map(|analysis| &analysis.spellings);
    let primary = analyses.first().cloned().map(|mut analysis| {
        analysis.analysis.class = AnalysisClass::Primary;
        analysis.analysis
    });
    let notes = respell_played_notes(&played.notes, primary_spellings);
    let aliases = analyses
        .into_iter()
        .skip(1)
        .map(|mut analysis| {
            analysis.analysis.class = classify_alias(&analysis.analysis, primary_score);
            analysis.analysis
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
    max_formula_pitches: usize,
}

impl CandidateStore {
    fn exact_matches(&self, set: PitchSet) -> impl Iterator<Item = &CandidateRecord> {
        self.exact[usize::from(set.bits)]
            .iter()
            .map(|idx| &self.records[*idx])
    }

    fn for_each_superset_match(&self, set: PitchSet, mut handle: impl FnMut(&CandidateRecord)) {
        for key in pitch_superset_keys(set, self.max_formula_pitches) {
            for idx in &self.exact[usize::from(key.bits)] {
                handle(&self.records[*idx]);
            }
        }
    }
}

fn candidate_store() -> &'static CandidateStore {
    static CANDIDATE_STORE: OnceLock<CandidateStore> = OnceLock::new();
    CANDIDATE_STORE.get_or_init(build_candidate_store)
}

fn build_candidate_store() -> CandidateStore {
    let records = build_candidate_records();
    let mut exact = array::from_fn(|_| Vec::new());
    let max_formula_pitches = records
        .iter()
        .map(|record| record.formula_set.len())
        .max()
        .unwrap_or(0);

    for (idx, record) in records.iter().enumerate() {
        exact[usize::from(record.formula_set.bits)].push(idx);
    }

    CandidateStore {
        records,
        exact,
        max_formula_pitches,
    }
}

fn pitch_superset_keys(set: PitchSet, max_pitches: usize) -> BoundedSupersetKeys {
    BoundedSupersetKeys::new(set, max_pitches)
}

struct BoundedSupersetKeys {
    base_bits: u16,
    positions: [u8; 12],
    position_count: usize,
    max_extra: usize,
    size: usize,
    combination: [usize; 12],
    started_size: bool,
    done: bool,
}

impl BoundedSupersetKeys {
    fn new(set: PitchSet, max_pitches: usize) -> Self {
        let mut positions = [0u8; 12];
        let mut position_count = 0;
        let variable_bits = !set.bits & 0x0fff;
        for bit in 0..12 {
            if variable_bits & (1u16 << bit) != 0 {
                positions[position_count] = bit;
                position_count += 1;
            }
        }

        let base_count = set.len();
        let done = base_count > max_pitches;
        Self {
            base_bits: set.bits,
            positions,
            position_count,
            max_extra: max_pitches.saturating_sub(base_count).min(position_count),
            size: 0,
            combination: [0; 12],
            started_size: false,
            done,
        }
    }

    fn current_key(&self) -> PitchSet {
        let mut bits = self.base_bits;
        for idx in self.combination.iter().take(self.size) {
            bits |= 1u16 << self.positions[*idx];
        }
        PitchSet { bits }
    }

    fn start_size(&mut self) {
        for idx in 0..self.size {
            self.combination[idx] = idx;
        }
        self.started_size = true;
    }

    fn advance(&mut self) {
        if self.size == 0 {
            self.size += 1;
            self.started_size = false;
            self.done = self.size > self.max_extra;
            return;
        }

        for idx in (0..self.size).rev() {
            let limit = self.position_count - self.size + idx;
            if self.combination[idx] < limit {
                self.combination[idx] += 1;
                for next in idx + 1..self.size {
                    self.combination[next] = self.combination[next - 1] + 1;
                }
                return;
            }
        }

        self.size += 1;
        self.started_size = false;
        self.done = self.size > self.max_extra;
    }
}

impl Iterator for BoundedSupersetKeys {
    type Item = PitchSet;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if !self.started_size {
            self.start_size();
        }
        let key = self.current_key();
        self.advance();
        Some(key)
    }
}

fn build_candidate_records() -> Vec<CandidateRecord> {
    let mut records = Vec::new();
    for root in candidate_root_spellings() {
        for spec in candidate_specs() {
            let formula = ChordFormula::from_parts(*root, spec);
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
            let mut respelled = played.clone();
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
    specs.sort_by_key(ChordSpec::suffix);
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

pub(crate) fn inferred_omissions(missing: PitchSet, formula: &ChordFormula) -> Option<Vec<String>> {
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
        push_unique_omission(&mut omissions, tone.degree);
    }

    if omissions.is_empty() {
        return None;
    }

    if formula.pitch_set().difference(missing).len() < 2 {
        return None;
    }

    Some(
        omissions
            .into_iter()
            .map(|degree| degree.to_string())
            .collect(),
    )
}

fn push_unique_omission(values: &mut Vec<u8>, value: u8) {
    if !values.contains(&value) {
        values.push(value);
        values.sort_unstable();
    }
}

fn can_infer_omission(tone: &ChordTone, formula: &ChordFormula) -> bool {
    match tone.degree {
        1 => formula.tones.len() >= 4,
        3 => formula.tones.len() >= 4 && formula_has_intervals(formula, &["b7"]),
        5 => tone.interval.is_natural_fifth(),
        _ => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuiltAnalysis {
    analysis: ChordAnalysis,
    spellings: [Option<NoteName>; 12],
}

fn build_analysis(
    root: NoteName,
    spec: &ChordSpec,
    formula: &ChordFormula,
    bass: PitchClass,
    omissions: Vec<String>,
    omitted: bool,
    show_chord_tone_bass: bool,
) -> BuiltAnalysis {
    let bass_name = formula
        .tone_for_pitch(bass)
        .map(|tone| tone.note)
        .unwrap_or_else(|| slash_bass_name(root, bass));
    let mut spellings = [None::<NoteName>; 12];
    for tone in &formula.tones {
        spellings[usize::from(tone.pitch_class)] = Some(tone.note);
    }
    spellings[usize::from(bass.value())] = Some(bass_name);

    let mut symbol = format!("{}{}", root, spec.suffix());
    if !omissions.is_empty() {
        for omission in omissions.iter().filter(|omission| omission.as_str() != "5") {
            symbol.push_str("no");
            symbol.push_str(omission);
        }
    }
    if bass != root.pitch_class() && show_chord_tone_bass {
        symbol.push('/');
        let _ = write!(&mut symbol, "{bass_name}");
    }

    let mut score = analysis_score(root, spec, formula, bass, omitted, show_chord_tone_bass);
    for omission in &omissions {
        score += omission_score(omission, formula);
    }
    score = score.saturating_sub(contextual_omission_adjustment(formula, &omissions));

    BuiltAnalysis {
        analysis: ChordAnalysis {
            symbol,
            root: root.to_string(),
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
            omissions,
            confidence: if omitted {
                Confidence::Omitted
            } else {
                Confidence::Exact
            },
            class: AnalysisClass::TheoreticalAlias,
            score,
        },
        spellings,
    }
}

fn build_slash_bass_analysis(
    root: NoteName,
    spec: &ChordSpec,
    formula: &ChordFormula,
    bass: PitchClass,
    omissions: Vec<String>,
    omitted: bool,
) -> BuiltAnalysis {
    let only_fifth_omitted = omissions == ["5"];
    let mut analysis = build_analysis(root, spec, formula, bass, omissions, omitted, true);
    let interval = slash_bass_interval(root, bass);
    analysis.analysis.score = match (interval, omitted, only_fifth_omitted) {
        (_, false, _) => analysis
            .analysis
            .score
            .saturating_sub(exact_slash_bass_offset(spec))
            .saturating_add(exact_slash_bass_penalty(interval)),
        (2, true, true) => analysis.analysis.score.saturating_sub(520),
        (1 | 3 | 6 | 8, true, true) => analysis.analysis.score.saturating_sub(400),
        (5, true, _) => analysis.analysis.score.saturating_add(220),
        _ => analysis.analysis.score.saturating_add(260),
    };
    analysis
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

fn omission_score(omission: &str, formula: &ChordFormula) -> u32 {
    match omission {
        "1" => {
            if formula_supports_rootless_upper_alias(formula) {
                100
            } else {
                500
            }
        }
        "3" => {
            if formula.tones.len() == 4 && formula_has_intervals(formula, &["b7"]) {
                420
            } else {
                500
            }
        }
        "5" => {
            if formula_has_upper_structure(formula) {
                40
            } else {
                80
            }
        }
        _ => 200,
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

fn contextual_omission_adjustment(formula: &ChordFormula, omissions: &[String]) -> u32 {
    if is_contextual_altered_upper_alias(formula, omissions) {
        650
    } else if is_contextual_rootless_half_diminished_alias(formula, omissions) {
        500
    } else if is_contextual_rootless_diminished_seventh_alias(formula, omissions) {
        420
    } else if is_contextual_rootless_upper_alias(formula, omissions) {
        if omissions.iter().any(|omission| omission == "5") {
            220
        } else {
            100
        }
    } else {
        0
    }
}

fn is_contextual_rootless_upper_alias(formula: &ChordFormula, omissions: &[String]) -> bool {
    omissions.iter().any(|omission| omission == "1")
        && omissions
            .iter()
            .all(|omission| matches!(omission.as_str(), "1" | "5"))
        && formula.tones.len().saturating_sub(omissions.len()) >= 4
        && !formula_has_altered_fifth(formula)
        && formula_supports_rootless_upper_alias(formula)
}

fn is_contextual_altered_upper_alias(formula: &ChordFormula, omissions: &[String]) -> bool {
    omissions.iter().any(|omission| omission == "1")
        && omissions.iter().any(|omission| omission == "3")
        && omissions
            .iter()
            .all(|omission| matches!(omission.as_str(), "1" | "3"))
        && formula_has_intervals(formula, &["b7", "b9", "#9", "b13"])
}

fn is_contextual_rootless_diminished_seventh_alias(
    formula: &ChordFormula,
    omissions: &[String],
) -> bool {
    omissions == ["1"] && formula_has_intervals(formula, &["b3", "b5", "bb7"])
}

fn is_contextual_rootless_half_diminished_alias(
    formula: &ChordFormula,
    omissions: &[String],
) -> bool {
    omissions == ["1"]
        && formula.tones.len() == 4
        && formula_has_intervals(formula, &["b3", "b5", "b7"])
}

fn formula_supports_rootless_upper_alias(formula: &ChordFormula) -> bool {
    formula_has_upper_structure(formula)
        && formula.tones.iter().any(|tone| tone.degree == 7)
        && formula
            .tones
            .iter()
            .any(|tone| tone.degree == 3 && tone.interval.to_string() == "3")
}

fn formula_has_upper_structure(formula: &ChordFormula) -> bool {
    formula
        .tones
        .iter()
        .any(|tone| matches!(tone.degree, 9 | 11 | 13))
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
            .any(|tone| tone.interval.to_string() == *interval)
    })
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
