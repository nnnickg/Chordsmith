use serde::Serialize;

use crate::formula::ChordFormula;
use crate::identify::inferred_omissions;
use crate::notes::{Fingering, GuitarTuning, NoteName, PitchClass, PitchSet, STANDARD_TUNING};
use crate::scoring::{rank_voicing_candidates, voicing_score};
use crate::symbol::ChordSymbol;
use crate::{
    ChordsmithError, DEFAULT_LIMIT, DEFAULT_MAX_FRET, DEFAULT_MAX_SPAN, DEFAULT_MIN_FRET,
    MAX_ALL_VOICINGS, MAX_DIVERSITY_SCORE_WINDOW, MAX_LIMIT, MAX_STANDARD_FRET, MAX_STRING_COUNT,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VoicingMode {
    Curated { limit: usize },
    All,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct VoicingOptions {
    pub min_fret: u8,
    pub max_fret: u8,
    pub max_span: u8,
    pub mode: VoicingMode,
}

impl Default for VoicingOptions {
    fn default() -> Self {
        Self {
            min_fret: DEFAULT_MIN_FRET,
            max_fret: DEFAULT_MAX_FRET,
            max_span: DEFAULT_MAX_SPAN,
            mode: VoicingMode::Curated {
                limit: DEFAULT_LIMIT,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Voicing {
    pub compact: String,
    pub dashed: String,
    pub frets: Vec<Option<u8>>,
    pub notes: Vec<String>,
    pub omissions: Vec<String>,
    pub score: u32,
}

pub fn voicings(input: &str, options: VoicingOptions) -> Result<Vec<Voicing>, ChordsmithError> {
    voicings_with_tuning(input, STANDARD_TUNING, options)
}

pub fn voicings_with_tuning(
    input: &str,
    tuning: GuitarTuning,
    options: VoicingOptions,
) -> Result<Vec<Voicing>, ChordsmithError> {
    if options.min_fret > MAX_STANDARD_FRET {
        return Err(ChordsmithError::new(format!(
            "invalid min_fret '{}': standard guitar range is 0..={MAX_STANDARD_FRET}",
            options.min_fret
        )));
    }
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
    if options.min_fret > options.max_fret {
        return Err(ChordsmithError::new(format!(
            "invalid fret range: min_fret '{}' cannot exceed max_fret '{}'",
            options.min_fret, options.max_fret
        )));
    }
    if let VoicingMode::Curated { limit } = options.mode
        && limit > MAX_LIMIT
    {
        return Err(ChordsmithError::new(format!(
            "invalid limit '{limit}': curated voicing limit is 0..={MAX_LIMIT}; use --all for exhaustive output"
        )));
    }

    let chord = ChordSymbol::parse(input)?;
    if options.mode == (VoicingMode::Curated { limit: 0 }) {
        return Ok(Vec::new());
    }

    let formula = chord.formula();
    let formula_target = formula.pitch_set();
    let target = required_pitch_set(&chord, &formula);
    let bass_rule = bass_rule_for_chord(&chord, tuning);

    let mut per_string = Vec::new();
    for (string, open) in tuning.notes().iter().enumerate() {
        let mut choices = vec![None];
        for fret in options.min_fret..=options.max_fret {
            let pitch = open.pitch_class().transpose(i16::from(fret));
            if target.contains(pitch) {
                choices.push(Some(fret));
            }
        }
        choices.sort_by_key(|choice| choice_sort_key(*choice, string));
        per_string.push(choices);
    }
    let suffix_sets = suffix_pitch_sets(&per_string, tuning);
    let suffix_frets = suffix_fret_stats(&per_string);

    let mut frets = [None; MAX_STRING_COUNT];
    let search = VoicingSearch {
        per_string: &per_string,
        suffix_sets: &suffix_sets,
        suffix_min_non_open: &suffix_frets.min_non_open,
        suffix_has_open: &suffix_frets.has_open,
        target,
        formula_target,
        bass_rule,
        bass_spelling: chord.bass,
        formula: &formula,
        options,
        tuning,
    };
    let out = match options.mode {
        VoicingMode::All => {
            let mut collector = AllVoicingCollector::new(MAX_ALL_VOICINGS);
            enumerate_voicings(&search, 0, &mut frets, &mut collector);
            collector.finish()?
        }
        VoicingMode::Curated { limit } => {
            let mut collector = TopVoicingCollector::new(limit);
            enumerate_voicings(&search, 0, &mut frets, &mut collector);
            collector.finish()
        }
    };
    let ranked = rank_voicing_candidates(out, options);

    Ok(materialize_voicings(ranked, &search))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum BassRule {
    Required(PitchClass),
    Preferred(PitchClass),
    Any,
}

impl BassRule {
    pub(crate) const fn required(self) -> Option<PitchClass> {
        match self {
            Self::Required(pitch) => Some(pitch),
            Self::Preferred(_) | Self::Any => None,
        }
    }

    pub(crate) const fn preferred(self) -> Option<PitchClass> {
        match self {
            Self::Required(pitch) | Self::Preferred(pitch) => Some(pitch),
            Self::Any => None,
        }
    }
}

fn bass_rule_for_chord(chord: &ChordSymbol, tuning: GuitarTuning) -> BassRule {
    if let Some(bass) = chord.bass {
        return BassRule::Required(bass.pitch_class());
    }
    if chord.spec.omissions.contains(&1) || !tuning.prefers_root_bass() {
        BassRule::Any
    } else {
        BassRule::Preferred(chord.root.pitch_class())
    }
}

fn required_pitch_set(chord: &ChordSymbol, formula: &ChordFormula) -> PitchSet {
    let mut target = formula.pitch_set();
    if let Some(bass) = chord.bass {
        target.insert(bass.pitch_class());
    }
    target
}

struct VoicingSearch<'a> {
    per_string: &'a [Vec<Option<u8>>],
    suffix_sets: &'a [PitchSet],
    suffix_min_non_open: &'a [Option<u8>; MAX_STRING_COUNT + 1],
    suffix_has_open: &'a [bool; MAX_STRING_COUNT + 1],
    target: PitchSet,
    formula_target: PitchSet,
    bass_rule: BassRule,
    bass_spelling: Option<NoteName>,
    formula: &'a ChordFormula,
    options: VoicingOptions,
    tuning: GuitarTuning,
}

impl VoicingSearch<'_> {
    const fn string_count(&self) -> usize {
        self.tuning.string_count()
    }
}

fn enumerate_voicings(
    search: &VoicingSearch<'_>,
    string_idx: usize,
    frets: &mut [Option<u8>; MAX_STRING_COUNT],
    out: &mut impl VoicingCollector,
) {
    if string_idx == search.string_count() {
        if let Some(candidate) = build_voicing_candidate(*frets, search) {
            out.insert(candidate);
        }
        return;
    }

    if let Some(choices) = search.per_string.get(string_idx) {
        for choice in choices {
            if out.should_stop() {
                return;
            }
            frets[string_idx] = *choice;
            if !partial_voicing_can_complete(search, string_idx + 1, frets) {
                continue;
            }
            if partial_voicing_score_floor(search, string_idx + 1, frets) > out.score_ceiling() {
                continue;
            }
            enumerate_voicings(search, string_idx + 1, frets, out);
        }
    }
}

trait VoicingCollector {
    fn insert(&mut self, candidate: VoicingCandidate);

    fn should_stop(&self) -> bool {
        false
    }

    fn score_ceiling(&self) -> u32 {
        u32::MAX
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VoicingCandidate {
    pub(crate) frets: [Option<u8>; MAX_STRING_COUNT],
    pub(crate) string_count: usize,
    pub(crate) omissions: Vec<String>,
    pub(crate) score: u32,
}

struct AllVoicingCollector {
    limit: usize,
    voicings: Vec<VoicingCandidate>,
    exceeded: bool,
}

impl AllVoicingCollector {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            voicings: Vec::new(),
            exceeded: false,
        }
    }

    fn finish(self) -> Result<Vec<VoicingCandidate>, ChordsmithError> {
        if self.exceeded {
            Err(ChordsmithError::new(format!(
                "voicing result exceeds --all cap of {MAX_ALL_VOICINGS}; narrow the search with --min-fret, --max-fret, or --max-span"
            )))
        } else {
            Ok(self.voicings)
        }
    }
}

impl VoicingCollector for AllVoicingCollector {
    fn insert(&mut self, candidate: VoicingCandidate) {
        if self.voicings.len() >= self.limit {
            self.exceeded = true;
            return;
        }
        self.voicings.push(candidate);
    }

    fn should_stop(&self) -> bool {
        self.exceeded
    }
}

struct TopVoicingCollector {
    limit: usize,
    best_scores: Vec<u32>,
    retained: Vec<VoicingCandidate>,
    ceiling: u32,
}

impl TopVoicingCollector {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            best_scores: Vec::new(),
            retained: Vec::new(),
            ceiling: u32::MAX,
        }
    }

    fn finish(self) -> Vec<VoicingCandidate> {
        self.retained
    }

    fn update_best_scores(&mut self, score: u32) {
        if self.limit == 0 {
            return;
        }

        let idx = self.best_scores.partition_point(|value| *value <= score);
        if self.best_scores.len() < self.limit {
            self.best_scores.insert(idx, score);
        } else if idx < self.limit {
            self.best_scores.insert(idx, score);
            self.best_scores.pop();
        }
    }

    fn update_ceiling(&mut self) {
        // Exact bound for the greedy diversity ranker:
        // before pick N, the best remaining raw score cannot be greater than
        // the Nth raw score. Since diversity only considers candidates within
        // MAX_DIVERSITY_SCORE_WINDOW of that raw floor, anything above the
        // kth raw score plus the max window is unreachable for a k-item result.
        let next = if self.limit == 0 {
            0
        } else if self.best_scores.len() < self.limit {
            u32::MAX
        } else {
            self.best_scores[self.limit - 1].saturating_add(MAX_DIVERSITY_SCORE_WINDOW)
        };

        if next < self.ceiling {
            self.retained.retain(|candidate| candidate.score <= next);
        }
        self.ceiling = next;
    }
}

impl VoicingCollector for TopVoicingCollector {
    fn insert(&mut self, candidate: VoicingCandidate) {
        if self.limit == 0 {
            return;
        }
        self.update_best_scores(candidate.score);
        self.update_ceiling();
        if candidate.score <= self.ceiling {
            self.retained.push(candidate);
        }
    }

    fn score_ceiling(&self) -> u32 {
        self.ceiling
    }
}

fn choice_sort_key(choice: Option<u8>, string: usize) -> (u8, u8, usize) {
    match choice {
        Some(0) => (0, 0, string),
        Some(fret) => (1, fret, string),
        None => (2, 0, string),
    }
}

fn suffix_pitch_sets(per_string: &[Vec<Option<u8>>], tuning: GuitarTuning) -> Vec<PitchSet> {
    let string_count = tuning.string_count();
    let mut suffix = vec![PitchSet::empty(); string_count + 1];
    for string in (0..string_count).rev() {
        let mut set = suffix[string + 1];
        for fret in &per_string[string] {
            let Some(fret) = fret else {
                continue;
            };
            let pitch = tuning.pitch_at(string, *fret);
            set.insert(pitch);
        }
        suffix[string] = set;
    }
    suffix
}

struct SuffixFretStats {
    min_non_open: [Option<u8>; MAX_STRING_COUNT + 1],
    has_open: [bool; MAX_STRING_COUNT + 1],
}

fn suffix_fret_stats(per_string: &[Vec<Option<u8>>]) -> SuffixFretStats {
    let mut min_non_open = [None::<u8>; MAX_STRING_COUNT + 1];
    let mut has_open = [false; MAX_STRING_COUNT + 1];

    for string in (0..per_string.len()).rev() {
        min_non_open[string] = min_non_open[string + 1];
        has_open[string] = has_open[string + 1];

        for choice in &per_string[string] {
            match choice {
                Some(0) => has_open[string] = true,
                Some(fret) => {
                    min_non_open[string] =
                        Some(min_non_open[string].map_or(*fret, |current| current.min(*fret)));
                }
                None => {}
            }
        }
    }

    SuffixFretStats {
        min_non_open,
        has_open,
    }
}

fn partial_voicing_can_complete(
    search: &VoicingSearch<'_>,
    next_string: usize,
    frets: &[Option<u8>; MAX_STRING_COUNT],
) -> bool {
    if !partial_span_valid(frets, next_string, search.options.max_span) {
        return false;
    }

    let current = partial_pitch_set(frets, next_string, search.tuning);
    let available = current.union(search.suffix_sets[next_string]);
    let missing_required = search.target.difference(available);
    if missing_required
        .iter()
        .any(|pitch| !search.formula_target.contains(pitch))
    {
        return false;
    }

    let missing_formula = search.formula_target.difference(available);
    inferred_omissions(missing_formula, search.formula).is_some()
}

fn partial_voicing_score_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    frets: &[Option<u8>; MAX_STRING_COUNT],
) -> u32 {
    let ceiling = partial_base_score_floor(search, next_string, frets);
    ceiling.saturating_sub(max_possible_bonus(search, next_string, frets))
}

fn partial_base_score_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    frets: &[Option<u8>; MAX_STRING_COUNT],
) -> u32 {
    let mut non_open = [0u8; MAX_STRING_COUNT];
    let mut non_open_count = 0;
    let mut has_open = false;
    let mut active_count = 0usize;
    let mut pitch_counts = [0u8; 12];
    let mut previous_fretted = None::<u8>;
    let mut adjacent_jump_cost = 0u32;

    for (string, fret) in frets.iter().take(next_string).enumerate() {
        let Some(fret) = fret else {
            continue;
        };

        active_count += 1;
        let pitch = search.tuning.pitch_at(string, *fret);
        pitch_counts[usize::from(pitch.value())] += 1;

        if *fret == 0 {
            has_open = true;
        } else {
            non_open[non_open_count] = *fret;
            non_open_count += 1;
            if let Some(previous) = previous_fretted {
                let jump = fret.abs_diff(previous);
                if jump > 2 {
                    let excess = u32::from(jump - 2);
                    adjacent_jump_cost += excess * excess * 4;
                }
            }
            previous_fretted = Some(*fret);
        }
    }

    let current_non_open = &non_open[..non_open_count];
    let position = partial_position_cost_floor(search, next_string, current_non_open, has_open);
    let relative = partial_relative_fret_cost_floor(current_non_open);
    let span = partial_fret_span_cost_floor(current_non_open);
    let duplicates = pitch_counts
        .iter()
        .map(|count| count.saturating_sub(2))
        .map(|excess| u32::from(excess) * 8)
        .sum::<u32>();
    let internal_mutes = partial_internal_mutes(frets, next_string) * 4;
    position
        + relative
        + span
        + duplicates
        + adjacent_jump_cost
        + internal_mutes
        + partial_active_string_cost_floor(active_count, next_string, search.string_count())
}

fn partial_position_cost_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    current_non_open: &[u8],
    has_open: bool,
) -> u32 {
    let current_min = current_non_open.iter().copied().min();
    let future_min = min_future_non_open_fret(search, next_string);
    let Some(min) = current_min.into_iter().chain(future_min).min() else {
        return 0;
    };
    let multiplier = if has_open || suffix_can_play_open(search, next_string) {
        1
    } else {
        2
    };
    u32::from(min) * multiplier
}

fn partial_relative_fret_cost_floor(non_open: &[u8]) -> u32 {
    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    non_open
        .iter()
        .map(|fret| u32::from(fret.saturating_sub(*min)))
        .sum()
}

fn partial_fret_span_cost_floor(non_open: &[u8]) -> u32 {
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    let span = u32::from(max.saturating_sub(*min));
    span * span
}

fn partial_active_string_cost_floor(
    active_count: usize,
    next_string: usize,
    string_count: usize,
) -> u32 {
    active_string_floor(
        active_count + string_count.saturating_sub(next_string),
        string_count,
    )
}

fn active_string_floor(active_count: usize, string_count: usize) -> u32 {
    if active_count == string_count {
        return 0;
    }
    if active_count + 1 == string_count {
        return 2;
    }
    match active_count {
        0 => 120,
        1 => 90,
        2 => 45,
        3 => 16,
        _ => 5,
    }
}

fn min_future_non_open_fret(search: &VoicingSearch<'_>, next_string: usize) -> Option<u8> {
    search.suffix_min_non_open[next_string]
}

fn suffix_can_play_open(search: &VoicingSearch<'_>, next_string: usize) -> bool {
    search.suffix_has_open[next_string]
}

fn partial_internal_mutes(frets: &[Option<u8>; MAX_STRING_COUNT], next_string: usize) -> u32 {
    let first = frets.iter().take(next_string).position(Option::is_some);
    let last = frets.iter().take(next_string).rposition(Option::is_some);
    let (Some(first), Some(last)) = (first, last) else {
        return 0;
    };
    u32::try_from(
        frets
            .iter()
            .take(last + 1)
            .skip(first)
            .filter(|fret| fret.is_none())
            .count(),
    )
    .unwrap_or(0)
}

fn max_possible_bonus(
    search: &VoicingSearch<'_>,
    next_string: usize,
    frets: &[Option<u8>; MAX_STRING_COUNT],
) -> u32 {
    let current_has_open = frets
        .iter()
        .take(next_string)
        .flatten()
        .any(|fret| *fret == 0);
    let mut current_min = None::<u8>;
    let mut current_max = None::<u8>;
    for fret in frets.iter().take(next_string).flatten().copied() {
        if fret == 0 {
            continue;
        }
        current_min = Some(current_min.map_or(fret, |value| value.min(fret)));
        current_max = Some(current_max.map_or(fret, |value| value.max(fret)));
    }
    let internal_mutes = partial_internal_mutes(frets, next_string);
    let can_play_open = current_has_open || suffix_can_play_open(search, next_string);
    let first_played = frets
        .iter()
        .take(next_string)
        .enumerate()
        .find_map(|(string, fret)| fret.map(|fret| (string, fret)));

    let mut bonus = 0u32;
    if can_play_open && current_max.is_none_or(|max| max <= 3) {
        bonus += 22;
    }
    if first_played.is_none_or(|(_, fret)| fret == 0) {
        bonus += 18;
    }
    if first_played.is_none_or(|(_, fret)| fret == 0) {
        bonus += 20;
    }
    if !current_has_open && internal_mutes == 0 {
        bonus += 30;
    }
    if !current_has_open && internal_mutes == 0 {
        bonus += 12;
    }
    if !current_has_open
        && current_min.is_none_or(|min| {
            min <= 3
                || min_future_non_open_fret(search, next_string).is_some_and(|future| future <= 3)
        })
    {
        bonus += 8;
    }
    if !current_has_open && internal_mutes <= 1 {
        bonus += 14;
    }
    bonus
}

fn partial_span_valid(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    next_string: usize,
    max_span: u8,
) -> bool {
    let mut min = None::<u8>;
    let mut max = None::<u8>;
    for fret in frets.iter().take(next_string).flatten().copied() {
        if fret == 0 {
            continue;
        }
        min = Some(min.map_or(fret, |current| current.min(fret)));
        max = Some(max.map_or(fret, |current| current.max(fret)));
    }

    match (min, max) {
        (Some(min), Some(max)) => max.saturating_sub(min) <= max_span,
        _ => true,
    }
}

fn partial_pitch_set(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    next_string: usize,
    tuning: GuitarTuning,
) -> PitchSet {
    let mut set = PitchSet::empty();
    for (string, fret) in frets.iter().take(next_string).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = tuning.pitch_at(string, *fret);
        set.insert(pitch);
    }
    set
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct FretProfile {
    pub(crate) non_open: [u8; MAX_STRING_COUNT],
    pub(crate) non_open_count: usize,
    pub(crate) active_count: usize,
    pub(crate) has_open: bool,
}

impl FretProfile {
    pub(crate) fn non_open(&self) -> &[u8] {
        &self.non_open[..self.non_open_count]
    }
}

pub(crate) fn fret_profile(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    string_count: usize,
) -> FretProfile {
    let mut non_open = [0u8; MAX_STRING_COUNT];
    let mut non_open_count = 0;
    let mut active_count = 0;
    let mut has_open = false;

    for fret in frets.iter().take(string_count).flatten().copied() {
        active_count += 1;
        if fret == 0 {
            has_open = true;
        } else {
            non_open[non_open_count] = fret;
            non_open_count += 1;
        }
    }

    FretProfile {
        non_open,
        non_open_count,
        active_count,
        has_open,
    }
}

fn build_voicing_candidate(
    frets: [Option<u8>; MAX_STRING_COUNT],
    search: &VoicingSearch<'_>,
) -> Option<VoicingCandidate> {
    let profile = fret_profile(&frets, search.string_count());
    if profile.active_count == 0 {
        return None;
    }

    let non_open = profile.non_open();
    if let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max())
        && max.saturating_sub(*min) > search.options.max_span
    {
        return None;
    }

    let mut current = PitchSet::empty();
    let mut actual_bass = None;
    let mut actual_bass_pitch = i16::MAX;

    for (string, fret) in frets.iter().take(search.string_count()).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let absolute_pitch = search.tuning.absolute_pitch(string, *fret);
        let pitch = PitchClass::new(absolute_pitch);
        current.insert(pitch);
        if absolute_pitch < actual_bass_pitch {
            actual_bass_pitch = absolute_pitch;
            actual_bass = Some(pitch);
        }
    }

    let missing_required = search.target.difference(current);
    if missing_required
        .iter()
        .any(|pitch| !search.formula_target.contains(pitch))
    {
        return None;
    }
    let missing_formula = search.formula_target.difference(current);
    let omissions = inferred_omissions(missing_formula, search.formula)?;
    if let Some(expected_bass) = search.bass_rule.required()
        && actual_bass != Some(expected_bass)
    {
        return None;
    }

    let score = voicing_score(
        &frets,
        search.tuning,
        search.bass_rule,
        search.formula,
        &omissions,
        search.string_count(),
    );
    Some(VoicingCandidate {
        frets,
        string_count: search.string_count(),
        omissions,
        score,
    })
}

fn materialize_voicings(
    candidates: Vec<VoicingCandidate>,
    search: &VoicingSearch<'_>,
) -> Vec<Voicing> {
    candidates
        .into_iter()
        .map(|candidate| materialize_voicing(candidate, search))
        .collect()
}

fn materialize_voicing(candidate: VoicingCandidate, search: &VoicingSearch<'_>) -> Voicing {
    let frets = candidate.frets;
    let string_count = search.string_count();
    let fingering = Fingering {
        frets,
        string_count,
    };
    Voicing {
        compact: fingering.compact(),
        dashed: fingering.dashed(),
        frets: frets[..string_count].to_vec(),
        notes: voicing_notes(&frets, search),
        omissions: candidate.omissions,
        score: candidate.score,
    }
}

fn voicing_notes(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    search: &VoicingSearch<'_>,
) -> Vec<String> {
    let mut notes = Vec::new();
    for (string, fret) in frets.iter().take(search.string_count()).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = search.tuning.pitch_at(string, *fret);
        let note = search
            .formula
            .tone_for_pitch(pitch)
            .map(|tone| tone.note.to_string())
            .or_else(|| {
                search
                    .bass_spelling
                    .filter(|bass| bass.pitch_class() == pitch)
                    .map(|bass| bass.to_string())
            })
            .unwrap_or_else(|| NoteName::simple_for_pitch(pitch, false).to_string());
        notes.push(note);
    }
    notes
}
