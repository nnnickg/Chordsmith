use serde::Serialize;

use crate::formula::ChordFormula;
use crate::identify::{can_infer_omission, can_omit, inferred_omissions};
use crate::notes::{
    Fingering, GuitarTuning, Instrument, NoteName, PitchClass, PitchSet, STANDARD_TUNING,
};
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

    let mut non_omissible_pitches = target;
    for tone in formula.tones() {
        if can_infer_omission(tone, &formula) {
            non_omissible_pitches.bits &= !(1u16 << tone.pitch_class);
        }
    }

    let mut frets = [None; MAX_STRING_COUNT];
    let search = VoicingSearch {
        per_string: &per_string,
        suffix_sets: &suffix_sets,
        suffix_min_non_open: &suffix_frets.min_non_open,
        suffix_has_open: &suffix_frets.has_open,
        target,
        formula_target,
        non_omissible_pitches,
        bass_rule,
        bass_spelling: chord.bass,
        formula: &formula,
        options,
        tuning,
    };
    let out = match options.mode {
        VoicingMode::All => {
            let mut collector = AllVoicingCollector::new(MAX_ALL_VOICINGS);
            enumerate_voicings(
                &search,
                0,
                &mut frets,
                PartialVoicingState::default(),
                &mut collector,
            );
            collector.finish()?
        }
        VoicingMode::Curated { limit } => {
            let mut collector = TopVoicingCollector::new(limit);
            enumerate_voicings(
                &search,
                0,
                &mut frets,
                PartialVoicingState::default(),
                &mut collector,
            );
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
    non_omissible_pitches: PitchSet,
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
    state: PartialVoicingState,
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
            let next_state = state.advance(search, string_idx, *choice);
            if !partial_voicing_can_complete(search, string_idx + 1, &next_state) {
                continue;
            }
            if partial_voicing_score_floor(search, string_idx + 1, &next_state)
                > out.score_ceiling()
            {
                continue;
            }
            enumerate_voicings(search, string_idx + 1, frets, next_state, out);
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

    fn finish(mut self) -> Vec<VoicingCandidate> {
        let ceiling = self.ceiling;
        self.retained.retain(|candidate| candidate.score <= ceiling);
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct PartialVoicingState {
    pitch_set: PitchSet,
    active_count: usize,
    has_open: bool,
    pitch_counts: [u8; 12],
    non_open_count: usize,
    non_open_sum: u16,
    min_non_open: Option<u8>,
    max_non_open: Option<u8>,
    previous_fretted: Option<u8>,
    adjacent_jump_cost: u32,
    internal_mutes: u32,
    trailing_mutes_after_active: u8,
    first_played: Option<(usize, u8)>,
}

impl Default for PartialVoicingState {
    fn default() -> Self {
        Self {
            pitch_set: PitchSet::empty(),
            active_count: 0,
            has_open: false,
            pitch_counts: [0; 12],
            non_open_count: 0,
            non_open_sum: 0,
            min_non_open: None,
            max_non_open: None,
            previous_fretted: None,
            adjacent_jump_cost: 0,
            internal_mutes: 0,
            trailing_mutes_after_active: 0,
            first_played: None,
        }
    }
}

impl PartialVoicingState {
    fn advance(mut self, search: &VoicingSearch<'_>, string: usize, choice: Option<u8>) -> Self {
        let Some(fret) = choice else {
            if self.active_count > 0 {
                self.trailing_mutes_after_active =
                    self.trailing_mutes_after_active.saturating_add(1);
            }
            return self;
        };

        if self.active_count == 0 {
            self.first_played = Some((string, fret));
        } else {
            self.internal_mutes += u32::from(self.trailing_mutes_after_active);
            self.trailing_mutes_after_active = 0;
        }

        self.active_count += 1;
        let pitch = search.tuning.pitch_at(string, fret);
        self.pitch_set.insert(pitch);
        self.pitch_counts[usize::from(pitch.value())] += 1;

        if fret == 0 {
            self.has_open = true;
            return self;
        }

        self.non_open_count += 1;
        self.non_open_sum += u16::from(fret);
        self.min_non_open = Some(self.min_non_open.map_or(fret, |current| current.min(fret)));
        self.max_non_open = Some(self.max_non_open.map_or(fret, |current| current.max(fret)));
        if let Some(previous) = self.previous_fretted {
            let jump = fret.abs_diff(previous);
            if jump > 2 {
                let excess = u32::from(jump - 2);
                self.adjacent_jump_cost += excess * excess * 4;
            }
        }
        self.previous_fretted = Some(fret);
        self
    }

    fn span_valid(self, max_span: u8) -> bool {
        match (self.min_non_open, self.max_non_open) {
            (Some(min), Some(max)) => max.saturating_sub(min) <= max_span,
            _ => true,
        }
    }
}

fn partial_voicing_can_complete(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
) -> bool {
    if !state.span_valid(search.options.max_span) {
        return false;
    }

    let current = state.pitch_set;
    let remaining_strings = search.string_count() - next_string;
    let non_omissible_not_played_bits = search.non_omissible_pitches.bits & !current.bits;
    if non_omissible_not_played_bits.count_ones() as usize > remaining_strings {
        return false;
    }

    let available = current.union(search.suffix_sets[next_string]);
    let missing_required_bits = search.target.bits & !available.bits;
    if (missing_required_bits & !search.formula_target.bits) != 0 {
        return false;
    }

    let missing_formula = search.formula_target.difference(available);
    can_omit(missing_formula, search.formula)
}

fn partial_voicing_score_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
) -> u32 {
    let ceiling = partial_base_score_floor(search, next_string, state);
    ceiling.saturating_sub(max_possible_bonus(search, next_string, state))
}

fn partial_base_score_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
) -> u32 {
    let position = partial_position_cost_floor(search, next_string, state);
    let relative = partial_relative_fret_cost_floor(state);
    let span = partial_fret_span_cost_floor(state);
    let duplicates = state
        .pitch_counts
        .iter()
        .map(|count| count.saturating_sub(2))
        .map(|excess| u32::from(excess) * 8)
        .sum::<u32>();
    let internal_mutes = state.internal_mutes * 4;
    position
        + relative
        + span
        + duplicates
        + state.adjacent_jump_cost
        + internal_mutes
        + partial_active_string_cost_floor(state.active_count, next_string, search.string_count())
}

fn partial_position_cost_floor(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
) -> u32 {
    let current_min = state.min_non_open;
    let future_min = min_future_non_open_fret(search, next_string);
    let Some(min) = current_min.into_iter().chain(future_min).min() else {
        return 0;
    };
    let multiplier = if state.has_open || suffix_can_play_open(search, next_string) {
        1
    } else {
        2
    };
    u32::from(min) * multiplier
}

fn partial_relative_fret_cost_floor(state: &PartialVoicingState) -> u32 {
    let Some(min) = state.min_non_open else {
        return 0;
    };
    u32::from(state.non_open_sum) - u32::from(min) * state.non_open_count as u32
}

fn partial_fret_span_cost_floor(state: &PartialVoicingState) -> u32 {
    let (Some(min), Some(max)) = (state.min_non_open, state.max_non_open) else {
        return 0;
    };
    let span = u32::from(max.saturating_sub(min));
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

fn max_possible_bonus(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
) -> u32 {
    let can_play_open = state.has_open || suffix_can_play_open(search, next_string);

    let mut bonus = 0u32;
    // open_position_bonus upper bound.
    if can_play_open && state.max_non_open.is_none_or(|max| max <= 3) {
        bonus += 22;
    }
    // open_root_bass_bonus upper bound.
    if state.first_played.is_none_or(|(_, fret)| fret == 0) {
        bonus += 18;
    }
    // open_bass_grip_bonus upper bound.
    if state.first_played.is_none_or(|(_, fret)| fret == 0) {
        bonus += 20;
    }
    // closed_shape_bonus upper bound.
    if !state.has_open && state.internal_mutes == 0 {
        bonus += 30;
    }
    // barre_grip_bonus upper bound.
    if !state.has_open && state.internal_mutes == 0 {
        bonus += 12;
    }
    // compact_low_grip_bonus upper bound.
    if !state.has_open
        && state.min_non_open.is_none_or(|min| {
            min <= 3
                || min_future_non_open_fret(search, next_string).is_some_and(|future| future <= 3)
        })
    {
        bonus += 8;
    }
    // jazz_shell_bonus upper bound.
    if !state.has_open && state.internal_mutes <= 1 {
        bonus += 14;
    }
    bonus += max_possible_instrument_bonus(search, next_string, state, can_play_open);
    bonus
}

fn max_possible_instrument_bonus(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
    can_play_open: bool,
) -> u32 {
    match search.tuning.instrument() {
        Instrument::Guitar => {
            max_possible_guitar_instrument_bonus(search, next_string, state, can_play_open)
        }
        Instrument::Ukulele => {
            max_possible_ukulele_instrument_bonus(search, next_string, state, can_play_open)
        }
        Instrument::Guitar7 | Instrument::Guitar8 => 0,
    }
}

fn max_possible_guitar_instrument_bonus(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
    can_play_open: bool,
) -> u32 {
    if search.string_count() != 6 {
        return 0;
    }

    let remaining = search.string_count().saturating_sub(next_string);
    let max_active = state.active_count + remaining;
    if !can_play_open {
        return 0;
    }

    let mut bonus = 0;
    if max_active >= 4 && state.max_non_open.is_none_or(|max| max <= 3) {
        bonus += 8;
    }
    if max_active >= 5
        && state.max_non_open.is_none_or(|max| max <= 5)
        && formula_has_degree(search.formula, 9)
        && !formula_has_degree(search.formula, 7)
    {
        bonus += 24;
    }
    bonus
}

fn max_possible_ukulele_instrument_bonus(
    search: &VoicingSearch<'_>,
    next_string: usize,
    state: &PartialVoicingState,
    can_play_open: bool,
) -> u32 {
    if search.string_count() != 4 {
        return 0;
    }

    let remaining = search.string_count().saturating_sub(next_string);
    let max_active = state.active_count + remaining;
    if can_play_open
        && max_active == search.string_count()
        && state.max_non_open.is_none_or(|max| max <= 4)
    {
        12
    } else {
        0
    }
}

fn formula_has_degree(formula: &ChordFormula, degree: u8) -> bool {
    formula.tones().iter().any(|tone| tone.degree == degree)
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
    let mut notes = Vec::with_capacity(search.string_count());
    for (string, fret) in frets.iter().take(search.string_count()).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = search.tuning.pitch_at(string, *fret);
        notes.push(voicing_note_name(pitch, search).to_string());
    }
    notes
}

fn voicing_note_name(pitch: PitchClass, search: &VoicingSearch<'_>) -> NoteName {
    search
        .formula
        .tone_for_pitch(pitch)
        .map(|tone| tone.note)
        .or_else(|| {
            search
                .bass_spelling
                .filter(|bass| bass.pitch_class() == pitch)
        })
        .unwrap_or_else(|| NoteName::simple_for_pitch(pitch, false))
}
