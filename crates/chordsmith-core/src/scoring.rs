use crate::formula::ChordFormula;
use crate::identify::InferredOmission;
use crate::notes::{GuitarTuning, Instrument, PitchClass};
use crate::voicing::{
    BassRule, FretProfile, VoicingCandidate, VoicingMode, VoicingOptions, fret_profile,
};
use crate::{MAX_DIVERSITY_SCORE_WINDOW, MAX_STRING_COUNT};

pub(crate) fn voicing_score_with_profile(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    bass_rule: BassRule,
    formula: &ChordFormula,
    omissions: &[InferredOmission],
    string_count: usize,
    profile: &FretProfile,
) -> u32 {
    if profile.active_count == 0 {
        return u32::MAX;
    }

    let non_open = profile.non_open();
    let position_cost = position_cost(profile);
    let relative_cost = relative_fret_cost(profile);
    let span_cost = fret_span_cost(profile);
    let string_cost = active_string_cost(profile.active_count, string_count);
    let internal_mute_cost = internal_mute_cost(profile);
    let jump_cost = adjacent_fret_jump_cost(frets, string_count);
    let duplicate_cost = duplicate_pitch_cost(frets, tuning, string_count);
    let high_open_cost = high_open_mix_cost(frets, tuning.instrument(), profile, string_count);
    let low_open_gap_cost = low_open_gap_cost(frets, string_count);
    let preferred_bass_cost =
        preferred_bass_mismatch_cost(frets, tuning, bass_rule.preferred(), string_count);
    let harmonic_defect_cost = voicing_omission_cost(omissions, formula);
    let internal_mute_quality_cost =
        internal_mute_quality_cost(profile, formula, tuning.instrument());
    let trailing_mute_cost = trailing_mute_cost(formula, profile);
    let sparse_duplicate_cost =
        sparse_duplicate_pitch_cost(frets, tuning, formula, profile, string_count);
    let open_bypass_cost =
        open_chord_tone_bypass_cost(frets, tuning, formula, profile, string_count);
    let low_extension_cluster_cost =
        low_added_ninth_cluster_cost(frets, tuning, formula, string_count);
    let fingering_complexity_cost = fingering_complexity_cost(frets, string_count);
    let instrument_cost =
        instrument_voicing_cost(frets, tuning.instrument(), profile, string_count);
    let instrument_bonus =
        instrument_voicing_bonus(frets, tuning.instrument(), formula, profile, string_count);

    let mut score = position_cost
        + relative_cost
        + span_cost
        + string_cost
        + internal_mute_cost
        + jump_cost
        + duplicate_cost
        + high_open_cost
        + low_open_gap_cost
        + preferred_bass_cost;
    score = score.saturating_sub(open_position_bonus(profile, string_count));
    score = score.saturating_sub(open_root_bass_bonus(
        frets,
        tuning,
        bass_rule.preferred(),
        string_count,
    ));
    score = score.saturating_sub(open_bass_grip_bonus(frets, profile.has_open, string_count));
    score = score.saturating_sub(closed_shape_bonus(non_open, profile, string_count));
    score = score.saturating_sub(barre_grip_bonus(frets, non_open, profile, string_count));
    score = score.saturating_sub(compact_low_grip_bonus(profile, string_count));
    score = score.saturating_sub(jazz_shell_bonus(formula, profile));
    let score = score
        + harmonic_defect_cost
        + internal_mute_quality_cost
        + trailing_mute_cost
        + sparse_duplicate_cost
        + open_bypass_cost
        + low_extension_cluster_cost
        + fingering_complexity_cost
        + instrument_cost;
    score.saturating_sub(instrument_bonus)
}

pub(crate) fn rank_voicing_candidates(
    candidates: Vec<VoicingCandidate>,
    options: VoicingOptions,
) -> Vec<VoicingCandidate> {
    match options.mode {
        VoicingMode::All => rank_all_voicing_candidates(candidates),
        VoicingMode::Curated { limit } => rank_diverse_voicing_candidates(candidates, limit),
    }
}

fn rank_all_voicing_candidates(candidates: Vec<VoicingCandidate>) -> Vec<VoicingCandidate> {
    let mut ranked = candidates
        .into_iter()
        .map(RankedVoicingCandidate::new)
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(compare_ranked_voicing_candidates);
    ranked
        .into_iter()
        .map(RankedVoicingCandidate::into_candidate)
        .collect()
}

pub(crate) fn rank_diverse_voicing_candidates(
    candidates: Vec<VoicingCandidate>,
    limit: usize,
) -> Vec<VoicingCandidate> {
    if limit == 0 {
        return Vec::new();
    }

    let mut candidates_with_family: Vec<RankedVoicingCandidate> = candidates
        .into_iter()
        .map(RankedVoicingCandidate::new)
        .collect();
    candidates_with_family.sort_unstable_by(compare_ranked_voicing_candidates);

    let mut selected_count = 0usize;
    let mut first_unselected_idx = 0usize;
    let mut family_counts = [0u32; VOICING_FAMILY_COUNT];
    let mut position_counts = [0u32; POSITION_FAMILY_COUNT];

    while selected_count < limit && first_unselected_idx < candidates_with_family.len() {
        let best_raw_score = candidates_with_family[first_unselected_idx].candidate.score;
        let candidate_ceiling =
            best_raw_score.saturating_add(diversity_score_window(best_raw_score));

        let Some(best_idx) = candidates_with_family
            .iter()
            .enumerate()
            .skip(first_unselected_idx)
            .take_while(|(_, candidate)| candidate.candidate.score <= candidate_ceiling)
            .filter(|(_, candidate)| !candidate.selected)
            .min_by(|(_, left), (_, right)| {
                let left_family_count = family_counts[left.family.index()];
                let left_pos_count = position_counts[left.family.position.index()];
                let left_eff_score = left.candidate.score + left_family_count * 5 + left_pos_count;

                let right_family_count = family_counts[right.family.index()];
                let right_pos_count = position_counts[right.family.position.index()];
                let right_eff_score =
                    right.candidate.score + right_family_count * 5 + right_pos_count;

                left_eff_score
                    .cmp(&right_eff_score)
                    .then(left.candidate.score.cmp(&right.candidate.score))
                    .then_with(|| compare_ranked_voicing_candidates(left, right))
            })
            .map(|(idx, _)| idx)
        else {
            break;
        };

        let chosen = &mut candidates_with_family[best_idx];
        chosen.selected = true;
        family_counts[chosen.family.index()] += 1;
        position_counts[chosen.family.position.index()] += 1;
        selected_count += 1;
        while first_unselected_idx < candidates_with_family.len()
            && candidates_with_family[first_unselected_idx].selected
        {
            first_unselected_idx += 1;
        }
    }

    let mut selected = candidates_with_family
        .into_iter()
        .filter(|candidate| candidate.selected)
        .collect::<Vec<_>>();
    selected.sort_unstable_by(compare_ranked_voicing_candidates);
    selected
        .into_iter()
        .map(RankedVoicingCandidate::into_candidate)
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RankedVoicingCandidate {
    candidate: VoicingCandidate,
    family: VoicingFamily,
    position_tie_break: u8,
    active_count: usize,
    selected: bool,
}

impl RankedVoicingCandidate {
    fn new(candidate: VoicingCandidate) -> Self {
        let profile = fret_profile(&candidate.frets, candidate.string_count);
        let family = voicing_family(candidate.string_count, &profile);
        let position_tie_break = voicing_position_tie_break(candidate.string_count, &profile);
        Self {
            candidate,
            family,
            position_tie_break,
            active_count: profile.active_count,
            selected: false,
        }
    }

    fn into_candidate(self) -> VoicingCandidate {
        self.candidate
    }
}

fn compare_ranked_voicing_candidates(
    left: &RankedVoicingCandidate,
    right: &RankedVoicingCandidate,
) -> std::cmp::Ordering {
    left.candidate
        .score
        .cmp(&right.candidate.score)
        .then(left.position_tie_break.cmp(&right.position_tie_break))
        .then_with(|| right.active_count.cmp(&left.active_count))
        .then_with(|| compare_fingering_compact(left, right))
}

fn compare_fingering_compact(
    left: &RankedVoicingCandidate,
    right: &RankedVoicingCandidate,
) -> std::cmp::Ordering {
    let (left_key, left_len) =
        fingering_compact_key(&left.candidate.frets, left.candidate.string_count);
    let (right_key, right_len) =
        fingering_compact_key(&right.candidate.frets, right.candidate.string_count);
    left_key[..left_len].cmp(&right_key[..right_len])
}

fn voicing_position_tie_break(string_count: usize, profile: &FretProfile) -> u8 {
    let max_fret = profile.max_non_open.unwrap_or(0);
    let min_non_open = profile.min_non_open.unwrap_or(0);

    if profile.has_open && max_fret <= 3 && profile.active_count + 1 >= string_count {
        0
    } else if profile.has_open && max_fret <= 5 {
        1
    } else if !profile.has_open && min_non_open <= 5 && profile.active_count == string_count {
        2
    } else {
        3
    }
}

fn fingering_compact_key(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    string_count: usize,
) -> ([u8; 32], usize) {
    let mut out = [0; 32];
    let mut len = 0;
    let dashed = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret > 9);

    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        if dashed && idx > 0 {
            out[len] = b'-';
            len += 1;
        }
        match fret {
            Some(value) => {
                if *value >= 10 {
                    out[len] = b'0' + (*value / 10);
                    len += 1;
                    out[len] = b'0' + (*value % 10);
                    len += 1;
                } else {
                    out[len] = b'0' + *value;
                    len += 1;
                }
            }
            None => {
                out[len] = b'x';
                len += 1;
            }
        }
    }

    (out, len)
}

fn diversity_score_window(best_score: u32) -> u32 {
    let window = match best_score {
        0..=5 => 12,
        6..=20 => 10,
        _ => 8,
    };
    debug_assert!(window <= MAX_DIVERSITY_SCORE_WINDOW);
    window
}

const POSITION_BUCKET_COUNT: usize = 4;
const POSITION_FAMILY_COUNT: usize = 1 + POSITION_BUCKET_COUNT * 2;
const STRING_BAND_COUNT: usize = 5;
const STRING_DENSITY_COUNT: usize = 3;
const VOICING_FAMILY_COUNT: usize =
    POSITION_FAMILY_COUNT * STRING_BAND_COUNT * STRING_DENSITY_COUNT * 2;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct VoicingFamily {
    position: PositionFamily,
    string_band: StringBand,
    density: StringDensity,
    internal_mute: bool,
}

impl VoicingFamily {
    const fn index(self) -> usize {
        (((self.position.index() * STRING_BAND_COUNT + self.string_band.index())
            * STRING_DENSITY_COUNT
            + self.density.index())
            * 2)
            + self.internal_mute as usize
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionFamily {
    OpenPosition,
    OpenHigh(PositionBucket),
    Closed(PositionBucket),
}

impl PositionFamily {
    const fn index(self) -> usize {
        match self {
            Self::OpenPosition => 0,
            Self::OpenHigh(bucket) => 1 + bucket.index(),
            Self::Closed(bucket) => 1 + POSITION_BUCKET_COUNT + bucket.index(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionBucket {
    Low,
    Middle,
    High,
    Upper,
}

impl PositionBucket {
    const fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Middle => 1,
            Self::High => 2,
            Self::Upper => 3,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringBand {
    Full,
    Wide,
    Low,
    Middle,
    High,
}

impl StringBand {
    const fn index(self) -> usize {
        match self {
            Self::Full => 0,
            Self::Wide => 1,
            Self::Low => 2,
            Self::Middle => 3,
            Self::High => 4,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringDensity {
    Full,
    Four,
    Small,
}

impl StringDensity {
    const fn index(self) -> usize {
        match self {
            Self::Full => 0,
            Self::Four => 1,
            Self::Small => 2,
        }
    }
}

fn voicing_family(string_count: usize, profile: &FretProfile) -> VoicingFamily {
    let first = profile.first_played.unwrap_or(0);
    let last = profile.last_played.unwrap_or(0);
    let has_open = profile.has_open;
    let max_non_open = profile.max_non_open.unwrap_or(0);
    let min_non_open = profile.min_non_open.unwrap_or(0);

    let position = if has_open && max_non_open <= 4 {
        PositionFamily::OpenPosition
    } else if has_open {
        PositionFamily::OpenHigh(position_bucket(min_non_open))
    } else {
        PositionFamily::Closed(position_bucket(min_non_open))
    };

    let string_band = if first == 0 && last + 1 == string_count {
        StringBand::Full
    } else if string_count >= 5 && first <= 1 && last + 2 >= string_count {
        StringBand::Wide
    } else if last < string_count / 2 {
        StringBand::Low
    } else if first >= string_count / 2 {
        StringBand::High
    } else {
        StringBand::Middle
    };

    let density = if profile.active_count == string_count {
        StringDensity::Full
    } else if profile.active_count + 1 >= string_count && profile.active_count >= 3 {
        StringDensity::Four
    } else {
        StringDensity::Small
    };

    VoicingFamily {
        position,
        string_band,
        density,
        internal_mute: profile.internal_mutes > 0,
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

fn position_cost(profile: &FretProfile) -> u32 {
    let Some(min) = profile.min_non_open else {
        return 0;
    };
    let multiplier = if profile.has_open { 1 } else { 2 };
    u32::from(min) * multiplier
}

fn relative_fret_cost(profile: &FretProfile) -> u32 {
    let Some(min) = profile.min_non_open else {
        return 0;
    };
    u32::from(profile.non_open_sum) - u32::from(min) * profile.non_open_count as u32
}

fn fret_span_cost(profile: &FretProfile) -> u32 {
    let Some(span) = profile.fret_span() else {
        return 0;
    };
    u32::from(span) * u32::from(span)
}

fn active_string_cost(active_count: usize, string_count: usize) -> u32 {
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

fn internal_mute_cost(profile: &FretProfile) -> u32 {
    let count = profile.internal_mutes;
    if count == 0 {
        return 0;
    }

    let compact_closed = !profile.has_open
        && profile.active_count >= 4
        && profile.fret_span().is_some_and(|span| span <= 4)
        && count <= 1;
    if compact_closed {
        count * 4
    } else {
        count * 10
    }
}

fn adjacent_fret_jump_cost(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let mut previous = None;
    let mut cost = 0;
    for fret in frets
        .iter()
        .take(string_count)
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
    {
        if let Some(previous) = previous {
            let jump = fret.abs_diff(previous);
            if jump > 2 {
                let excess = u32::from(jump - 2);
                cost += excess * excess * 4;
            }
        }
        previous = Some(fret);
    }
    cost
}

fn duplicate_pitch_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    string_count: usize,
) -> u32 {
    let mut counts = [0u8; 12];
    for (string, fret) in frets.iter().take(string_count).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = tuning.pitch_at(string, *fret);
        counts[usize::from(pitch.value())] += 1;
    }

    counts
        .iter()
        .map(|count| count.saturating_sub(2))
        .map(|excess| u32::from(excess) * 8)
        .sum()
}

fn high_open_mix_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    instrument: Instrument,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    let (Some(min), Some(max)) = (profile.min_non_open, profile.max_non_open) else {
        return 0;
    };
    if max <= 3 {
        return 0;
    }

    if !profile.has_open {
        return 0;
    }

    if matches!(instrument, Instrument::Ukulele) && max <= 4 {
        return 0;
    }

    if compact_open_treble_grip(frets, profile, string_count) {
        return 0;
    }

    let mut first_fretted = None;
    let mut fretted_count = 0;
    let mut previous_fretted = None;
    let mut contiguous_fretted = true;
    let mut open_before_first_fretted = true;
    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        match *fret {
            Some(0) if first_fretted.is_some() => open_before_first_fretted = false,
            Some(value) if value > 0 => {
                first_fretted.get_or_insert(idx);
                if let Some(previous) = previous_fretted
                    && idx != previous + 1
                {
                    contiguous_fretted = false;
                }
                previous_fretted = Some(idx);
                fretted_count += 1;
            }
            _ => {}
        }
    }

    if open_before_first_fretted {
        let excess = u32::from(max.saturating_sub(5));
        if contiguous_fretted && fretted_count >= 4 {
            return excess * 2;
        }
        if contiguous_fretted && fretted_count >= 3 {
            return excess * excess * 2;
        }
        return excess * excess * 4;
    }

    let excess = u32::from(max.saturating_sub(3));
    let random_high_fret_cost = excess * excess * 16;
    let high_position_open_cost = if min >= 5 { 24 } else { 0 };
    random_high_fret_cost + high_position_open_cost
}

fn compact_open_treble_grip(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    profile: &FretProfile,
    string_count: usize,
) -> bool {
    if profile.active_count + 1 < string_count || profile.trailing_mutes > 0 {
        return false;
    }

    let Some(span) = profile.fret_span() else {
        return false;
    };
    let Some(min) = profile.min_non_open else {
        return false;
    };
    let Some(max) = profile.max_non_open else {
        return false;
    };
    if span > 4 || min > 5 || max > 7 {
        return false;
    }

    let mut first_fretted = None;
    let mut previous_fretted = None;
    let mut saw_open_after_fretted = false;
    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        match *fret {
            Some(0) if first_fretted.is_some() => saw_open_after_fretted = true,
            Some(value) if value > 0 => {
                if saw_open_after_fretted {
                    return false;
                }
                first_fretted.get_or_insert(idx);
                if previous_fretted.is_some_and(|previous| idx != previous + 1) {
                    return false;
                }
                previous_fretted = Some(idx);
            }
            _ => {}
        }
    }

    saw_open_after_fretted
}

fn low_open_gap_cost(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let Some((first_idx, first_fret)) = frets
        .iter()
        .take(string_count)
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|value| (idx, value)))
        .next()
    else {
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
        .take(string_count)
        .skip(first_idx + 2)
        .flatten()
        .any(|fret| *fret > 0);
    if immediate_open && fretted_above {
        18
    } else {
        0
    }
}

fn preferred_bass_mismatch_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    preferred_bass: Option<PitchClass>,
    string_count: usize,
) -> u32 {
    let Some(preferred_bass) = preferred_bass else {
        return 0;
    };
    let actual = frets
        .iter()
        .take(string_count)
        .enumerate()
        .filter_map(|(string, fret)| {
            fret.map(|fret| (tuning.absolute_pitch(string, fret), string, fret))
        })
        .min_by_key(|(absolute_pitch, _, _)| *absolute_pitch)
        .map(|(_, string, fret)| tuning.pitch_at(string, fret));
    if actual == Some(preferred_bass) {
        0
    } else {
        100
    }
}

fn voicing_omission_cost(omissions: &[InferredOmission], formula: &ChordFormula) -> u32 {
    let upper_structure = has_upper_structure(formula);
    omissions
        .iter()
        .map(|omission| match *omission {
            InferredOmission::Root => {
                if upper_structure {
                    45
                } else {
                    140
                }
            }
            InferredOmission::Third => 160,
            InferredOmission::Fifth => {
                if upper_structure {
                    0
                } else {
                    90
                }
            }
        })
        .sum()
}

fn internal_mute_quality_cost(
    profile: &FretProfile,
    formula: &ChordFormula,
    instrument: Instrument,
) -> u32 {
    let count = profile.internal_mutes;
    if count == 0 {
        return 0;
    }

    let compact_extended_shell = has_upper_structure(formula)
        && !profile.has_open
        && profile.active_count <= 4
        && profile.fret_span().is_some_and(|span| span <= 4);
    if compact_extended_shell {
        return 0;
    }

    if !has_upper_structure(formula) && profile.active_count >= 5 {
        return count * 36;
    }

    if profile.has_open {
        if matches!(instrument, Instrument::Ukulele) {
            count * 12
        } else {
            count * 24
        }
    } else {
        count * 20
    }
}

fn trailing_mute_cost(formula: &ChordFormula, profile: &FretProfile) -> u32 {
    let trailing = profile.trailing_mutes;
    if trailing == 0 {
        return 0;
    }

    let compact_extended_shell = has_upper_structure(formula) && profile.active_count <= 4;
    let unit = if compact_extended_shell {
        2
    } else if profile.active_count >= 5 {
        8
    } else {
        10
    };

    u32::try_from(trailing).unwrap_or(0) * unit
}

fn instrument_voicing_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    instrument: Instrument,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    if !matches!(instrument, Instrument::Ukulele) {
        return 0;
    }

    ukulele_high_position_cost(profile)
        + ukulele_treble_mute_cost(frets, profile.active_count, string_count)
}

fn instrument_voicing_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    instrument: Instrument,
    formula: &ChordFormula,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    match instrument {
        Instrument::Guitar => {
            guitar_open_position_bonus(profile, string_count)
                + guitar_open_treble_extension_bonus(frets, formula, profile, string_count)
        }
        Instrument::Ukulele => ukulele_diagonal_open_grip_bonus(profile, string_count),
        Instrument::Guitar7 | Instrument::Guitar8 => 0,
    }
}

fn guitar_open_position_bonus(profile: &FretProfile, string_count: usize) -> u32 {
    if string_count != 6 || !profile.has_open || profile.active_count < 4 {
        return 0;
    }

    let Some(max) = profile.max_non_open else {
        return 0;
    };
    if max <= 3 && profile.trailing_mutes == 0 {
        8
    } else {
        0
    }
}

fn guitar_open_treble_extension_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    formula: &ChordFormula,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    if string_count != 6
        || !profile.has_open
        || profile.active_count < 5
        || frets[string_count - 1] != Some(0)
        || !formula_has_degree(formula, 9)
        || formula_has_degree(formula, 7)
    {
        return 0;
    }

    let Some(span) = profile.fret_span() else {
        return 0;
    };
    let Some(max) = profile.max_non_open else {
        return 0;
    };
    if span <= 4 && max <= 5 { 24 } else { 0 }
}

fn ukulele_diagonal_open_grip_bonus(profile: &FretProfile, string_count: usize) -> u32 {
    let non_open = profile.non_open();
    if string_count != 4
        || !profile.has_open
        || profile.active_count != string_count
        || non_open.len() < 3
    {
        return 0;
    }

    let (Some(min), Some(max)) = (profile.min_non_open, profile.max_non_open) else {
        return 0;
    };
    if max > 4 || max.saturating_sub(min) + 1 != u8::try_from(non_open.len()).unwrap_or(0) {
        return 0;
    }

    let mut seen = [false; 5];
    for fret in non_open {
        let offset = usize::from(fret.saturating_sub(min));
        if offset >= seen.len() || seen[offset] {
            return 0;
        }
        seen[offset] = true;
    }

    12
}

fn ukulele_high_position_cost(profile: &FretProfile) -> u32 {
    let (Some(min), Some(max)) = (profile.min_non_open, profile.max_non_open) else {
        return 0;
    };

    if min >= 7 {
        let excess = u32::from(min.saturating_sub(6));
        return 36 + excess * excess * 3;
    }

    if max >= 7 && min <= 4 {
        let excess = u32::from(max.saturating_sub(6));
        return if profile.has_open {
            24 + excess * 4
        } else {
            18 + excess * 4
        };
    }

    0
}

fn ukulele_treble_mute_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    active_count: usize,
    string_count: usize,
) -> u32 {
    if string_count != 4 || frets[string_count - 1].is_some() {
        return 0;
    }

    if active_count + 1 == string_count {
        18
    } else {
        8
    }
}

fn sparse_duplicate_pitch_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    formula: &ChordFormula,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    let active_count = profile.active_count;
    let internal_mute_count = profile.internal_mutes;
    if active_count >= 5 && internal_mute_count == 0 {
        return 0;
    }

    let open_complete_grip = profile.has_open && internal_mute_count == 0;
    if active_count == string_count && open_complete_grip && profile.trailing_mutes == 0 {
        return 0;
    }

    let mut counts = [0u8; 12];
    for (string, fret) in frets.iter().take(string_count).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        let pitch = tuning.pitch_at(string, *fret);
        counts[usize::from(pitch.value())] += 1;
    }

    let duplicate_instances = counts
        .iter()
        .map(|count| count.saturating_sub(1))
        .map(u32::from)
        .sum::<u32>();
    if duplicate_instances == 0 {
        return 0;
    }

    let unit = if has_upper_structure(formula) {
        2
    } else if active_count >= 5 {
        6
    } else if active_count == 4 {
        8
    } else {
        14
    };
    duplicate_instances * unit
}

fn open_chord_tone_bypass_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    formula: &ChordFormula,
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    if !profile.has_open
        || dense_open_bass_grip(frets, string_count)
        || compact_open_treble_grip(frets, profile, string_count)
    {
        return 0;
    }

    let target = formula.pitch_set();
    frets
        .iter()
        .take(string_count)
        .enumerate()
        .filter_map(|(string, fret)| fret.map(|fret| (string, fret)))
        .filter(|(_, fret)| *fret > 3)
        .filter(|(string, _)| target.contains(tuning.pitch_at(*string, 0)))
        .map(|(_, fret)| {
            let excess = u32::from(fret.saturating_sub(3));
            excess * excess * 12
        })
        .sum()
}

fn low_added_ninth_cluster_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    formula: &ChordFormula,
    string_count: usize,
) -> u32 {
    if !formula_has_degree(formula, 9) {
        return 0;
    }

    let Some(root) = formula.tones().iter().find(|tone| tone.degree == 1) else {
        return 0;
    };
    let Some(ninth) = formula.tones().iter().find(|tone| tone.degree == 9) else {
        return 0;
    };

    let mut played = [0i16; MAX_STRING_COUNT];
    let mut played_count = 0;
    for (string, fret) in frets.iter().take(string_count).enumerate() {
        let Some(fret) = fret else {
            continue;
        };
        played[played_count] = tuning.absolute_pitch(string, *fret);
        played_count += 1;
    }
    if played_count < 2 {
        return 0;
    }
    played[..played_count].sort_unstable();

    let low = played[0];
    let next = played[1];
    let low_pitch = PitchClass::new(low);
    let next_pitch = PitchClass::new(next);
    if low_pitch.value() == root.pitch_class
        && next_pitch.value() == ninth.pitch_class
        && next.saturating_sub(low) <= 2
    {
        42
    } else {
        0
    }
}

fn fingering_complexity_cost(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let estimated = estimated_finger_count(frets, string_count);
    if estimated <= 4 {
        0
    } else {
        u32::try_from(estimated - 4).unwrap_or(0) * 30
    }
}

fn estimated_finger_count(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> usize {
    let fretted_count = frets
        .iter()
        .take(string_count)
        .flatten()
        .filter(|fret| **fret > 0)
        .count();
    if fretted_count <= 1 {
        return fretted_count;
    }

    let mut covered = [false; MAX_STRING_COUNT];
    let mut barre_blocked = [false; MAX_STRING_COUNT];
    let mut barre_count = 0;

    while let Some(barre) = best_barre(frets, &covered, &barre_blocked, string_count) {
        barre_count += 1;
        for (string, fret) in frets
            .iter()
            .enumerate()
            .take(barre.end + 1)
            .skip(barre.start)
        {
            if *fret == Some(barre.fret) {
                covered[string] = true;
            }
        }
        if barre.bridged_mute {
            for blocked in barre_blocked
                .iter_mut()
                .take(barre.end + 1)
                .skip(barre.start)
            {
                *blocked = true;
            }
        }
    }

    let covered_count = covered.into_iter().filter(|value| *value).count();
    barre_count + fretted_count.saturating_sub(covered_count)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct BarreCandidate {
    fret: u8,
    start: usize,
    end: usize,
    saving: usize,
    bridged_mute: bool,
}

fn best_barre(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    covered: &[bool; MAX_STRING_COUNT],
    barre_blocked: &[bool; MAX_STRING_COUNT],
    string_count: usize,
) -> Option<BarreCandidate> {
    let mut best = None;
    let mut unique_frets = [0u8; MAX_STRING_COUNT];
    let mut unique_count = 0;
    for fret in frets.iter().take(string_count).flatten().copied() {
        if fret > 0 && !unique_frets[..unique_count].contains(&fret) {
            unique_frets[unique_count] = fret;
            unique_count += 1;
        }
    }
    for &fret in &unique_frets[..unique_count] {
        for start in 0..string_count {
            if frets[start] != Some(fret) || covered[start] || barre_blocked[start] {
                continue;
            }
            for end in (start + 1)..string_count {
                if frets[end] != Some(fret) || covered[end] || barre_blocked[end] {
                    continue;
                }
                let Some(range) = barre_range(frets, fret, start, end) else {
                    continue;
                };

                let exact_count = (start..=end)
                    .filter(|string| {
                        frets[*string] == Some(fret) && !covered[*string] && !barre_blocked[*string]
                    })
                    .count();
                if exact_count < 2 {
                    continue;
                }

                let candidate = BarreCandidate {
                    fret,
                    start,
                    end,
                    saving: exact_count - 1,
                    bridged_mute: range.bridged_mute,
                };
                if best.is_none_or(|current: BarreCandidate| {
                    candidate
                        .saving
                        .cmp(&current.saving)
                        .then((candidate.end - candidate.start).cmp(&(current.end - current.start)))
                        .is_gt()
                }) {
                    best = Some(candidate);
                }
            }
        }
    }

    best
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct BarreRange {
    bridged_mute: bool,
}

fn barre_range(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    fret: u8,
    start: usize,
    end: usize,
) -> Option<BarreRange> {
    let mut exact_count = 0usize;
    let mut higher_count = 0usize;
    let mut muted_count = 0usize;

    for value in frets.iter().take(end + 1).skip(start) {
        match value {
            Some(value) if *value == fret => exact_count += 1,
            Some(value) if *value > fret => higher_count += 1,
            Some(_) => return None,
            None => muted_count += 1,
        }
    }

    if muted_count == 0 {
        Some(BarreRange {
            bridged_mute: false,
        })
    } else if exact_count >= 3 && higher_count > 0 && muted_count <= exact_count {
        Some(BarreRange { bridged_mute: true })
    } else {
        None
    }
}

fn dense_open_bass_grip(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> bool {
    let mut has_open = false;
    let mut first_fretted = None;
    let mut previous_fretted = None;
    let mut fretted_count = 0;
    let mut min_non_open = None::<u8>;
    let mut max_non_open = None::<u8>;

    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        match fret {
            Some(0) => {
                if first_fretted.is_some() {
                    return false;
                }
                has_open = true;
            }
            Some(value) => {
                first_fretted.get_or_insert(idx);
                if previous_fretted.is_some_and(|previous| idx != previous + 1) {
                    return false;
                }
                previous_fretted = Some(idx);
                fretted_count += 1;
                min_non_open = Some(min_non_open.map_or(*value, |current| current.min(*value)));
                max_non_open = Some(max_non_open.map_or(*value, |current| current.max(*value)));
            }
            None => {}
        }
    }

    let min_fretted = if string_count >= 6 {
        4
    } else {
        string_count.saturating_sub(1)
    };
    if !has_open || fretted_count < min_fretted {
        return false;
    }

    let (Some(min), Some(max)) = (min_non_open, max_non_open) else {
        return false;
    };
    min >= 5 && max.saturating_sub(min) <= 4
}

fn has_upper_structure(formula: &ChordFormula) -> bool {
    formula
        .tones
        .iter()
        .any(|tone| matches!(tone.degree, 6 | 7 | 9 | 11 | 13))
}

fn formula_has_degree(formula: &ChordFormula, degree: u8) -> bool {
    formula.tones.iter().any(|tone| tone.degree == degree)
}

fn open_position_bonus(profile: &FretProfile, string_count: usize) -> u32 {
    let max_fret = profile.max_non_open.unwrap_or(0);
    let min_active = if string_count >= 6 {
        4
    } else {
        string_count.saturating_sub(1)
    };
    if profile.has_open && max_fret <= 3 && profile.active_count >= min_active {
        if profile.active_count == string_count {
            22
        } else if profile.active_count + 1 == string_count {
            10
        } else {
            4
        }
    } else {
        0
    }
}

fn open_root_bass_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: &GuitarTuning,
    expected_bass: Option<PitchClass>,
    string_count: usize,
) -> u32 {
    let Some(expected_bass) = expected_bass else {
        return 0;
    };

    let mut played_count = 0;
    let mut lowest_played = None;
    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        let Some(value) = fret else {
            continue;
        };
        played_count += 1;
        let absolute_pitch = tuning.absolute_pitch(idx, *value);
        if lowest_played.is_none_or(|(current, _, _)| absolute_pitch < current) {
            lowest_played = Some((absolute_pitch, idx, *value));
        }
    }

    let Some((_, lowest_string, lowest_fret)) = lowest_played else {
        return 0;
    };
    if lowest_fret != 0 {
        return 0;
    }

    let bass = tuning.pitch_at(lowest_string, lowest_fret);
    if bass != expected_bass {
        return 0;
    }

    let max_fret = frets
        .iter()
        .take(string_count)
        .flatten()
        .copied()
        .max()
        .unwrap_or(0);
    if max_fret > 4 {
        return 0;
    }

    if string_count >= 6 {
        match played_count {
            5.. => 18,
            4 => 14,
            _ => 8,
        }
    } else if played_count == string_count {
        18
    } else if played_count + 1 == string_count {
        14
    } else {
        8
    }
}

fn open_bass_grip_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    has_open: bool,
    string_count: usize,
) -> u32 {
    let mut first_fretted = None;
    let mut last_fretted = None;
    let mut fretted_count = 0;
    let mut min_non_open = None::<u8>;
    let mut max_non_open = None::<u8>;

    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        match fret {
            Some(0) if first_fretted.is_some() => return 0,
            Some(0) => {}
            Some(value) => {
                first_fretted.get_or_insert(idx);
                last_fretted = Some(idx);
                fretted_count += 1;
                min_non_open = Some(min_non_open.map_or(*value, |current| current.min(*value)));
                max_non_open = Some(max_non_open.map_or(*value, |current| current.max(*value)));
            }
            None => {}
        }
    }

    let (Some(first), Some(last), Some(min), Some(max)) =
        (first_fretted, last_fretted, min_non_open, max_non_open)
    else {
        return 0;
    };

    if !has_open || !(first..=last).all(|idx| frets[idx].is_some()) || min < 5 {
        return 0;
    }
    if max.saturating_sub(min) > 4 {
        return 0;
    }

    if fretted_count + 1 >= string_count {
        20
    } else {
        10
    }
}

fn closed_shape_bonus(non_open: &[u8], profile: &FretProfile, string_count: usize) -> u32 {
    let min_active = if string_count >= 6 {
        4
    } else {
        string_count.saturating_sub(1)
    };
    if profile.has_open || profile.active_count < min_active || profile.internal_mutes > 0 {
        return 0;
    }

    let (Some(min), Some(max)) = (profile.min_non_open, profile.max_non_open) else {
        return 0;
    };
    let min_count = non_open.iter().filter(|fret| **fret == min).count();
    if min_count < 2 {
        return 0;
    }

    if max.saturating_sub(min) <= 2 {
        let base = match min {
            0..=2 => 30,
            3..=5 => 6,
            6..=8 => 12,
            _ => 8,
        };
        if profile.active_count == string_count {
            base
        } else {
            base / 2
        }
    } else {
        0
    }
}

fn barre_grip_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    profile: &FretProfile,
    string_count: usize,
) -> u32 {
    if profile.has_open || profile.active_count != string_count || profile.internal_mutes > 0 {
        return 0;
    }

    let Some(span) = profile.fret_span() else {
        return 0;
    };
    if span > 4 {
        return 0;
    }

    let Some(min) = profile.min_non_open else {
        return 0;
    };
    let min_count = non_open.iter().filter(|fret| **fret == min).count();
    if min_count < 2 {
        return 0;
    }

    let reaches_treble = frets[string_count - 1].is_some();
    if !reaches_treble {
        return 0;
    }

    match min {
        0..=5 => 12,
        6..=8 => 4,
        _ => 2,
    }
}

fn compact_low_grip_bonus(profile: &FretProfile, string_count: usize) -> u32 {
    if profile.has_open || profile.active_count + 1 < string_count {
        return 0;
    }

    let Some(span) = profile.fret_span() else {
        return 0;
    };
    let min = profile.min_non_open.unwrap_or(0);
    if min <= 3 && span <= 2 { 8 } else { 0 }
}

fn jazz_shell_bonus(formula: &ChordFormula, profile: &FretProfile) -> u32 {
    if !has_upper_structure(formula) {
        return 0;
    }

    if profile.has_open || !(3..=4).contains(&profile.active_count) {
        return 0;
    }

    let Some(span) = profile.fret_span() else {
        return 0;
    };
    if span > 4 {
        return 0;
    }

    let muted = profile.internal_mutes;
    if muted > 1 {
        return 0;
    }

    match profile.active_count {
        4 => {
            if muted == 1 {
                14
            } else {
                10
            }
        }
        3 => 6,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frets(values: &[Option<u8>]) -> [Option<u8>; MAX_STRING_COUNT] {
        let mut out = [None; MAX_STRING_COUNT];
        for (idx, value) in values.iter().copied().enumerate() {
            out[idx] = value;
        }
        out
    }

    #[test]
    fn finger_estimate_allows_real_barres_across_muted_strings() {
        let f_with_muted_g = frets(&[Some(1), Some(3), Some(3), None, Some(1), Some(1)]);
        assert_eq!(estimated_finger_count(&f_with_muted_g, 6), 3);

        let partial_f_with_muted_g = frets(&[Some(1), Some(3), Some(1), None, Some(1), Some(1)]);
        assert_eq!(estimated_finger_count(&partial_f_with_muted_g, 6), 2);
    }

    #[test]
    fn finger_estimate_does_not_treat_sparse_endpoints_as_a_barre() {
        let sparse = frets(&[Some(1), None, None, None, None, Some(1)]);
        assert_eq!(estimated_finger_count(&sparse, 6), 2);
    }
}
