use crate::formula::ChordFormula;
use crate::notes::{GuitarTuning, Instrument, PitchClass};
use crate::voicing::{BassRule, VoicingCandidate, VoicingMode, VoicingOptions, fret_profile};
use crate::{MAX_DIVERSITY_SCORE_WINDOW, MAX_STANDARD_FRET, MAX_STRING_COUNT};

pub(crate) fn voicing_score(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: GuitarTuning,
    bass_rule: BassRule,
    formula: &ChordFormula,
    omissions: &[String],
    string_count: usize,
) -> u32 {
    let profile = fret_profile(frets, string_count);
    if profile.active_count == 0 {
        return u32::MAX;
    }

    let non_open = profile.non_open();
    let position_cost = position_cost(non_open, profile.has_open);
    let relative_cost = relative_fret_cost(non_open);
    let span_cost = fret_span_cost(non_open);
    let string_cost = active_string_cost(profile.active_count, string_count);
    let internal_mute_cost =
        internal_mute_cost(frets, non_open, profile.active_count, string_count);
    let jump_cost = adjacent_fret_jump_cost(frets, string_count);
    let duplicate_cost = duplicate_pitch_cost(frets, tuning, string_count);
    let high_open_cost = high_open_mix_cost(frets, tuning.instrument(), string_count);
    let low_open_gap_cost = low_open_gap_cost(frets, string_count);
    let preferred_bass_cost =
        preferred_bass_mismatch_cost(frets, tuning, bass_rule.preferred(), string_count);
    let harmonic_defect_cost = voicing_omission_cost(omissions, formula);
    let internal_mute_quality_cost =
        internal_mute_quality_cost(frets, non_open, formula, tuning.instrument(), string_count);
    let trailing_mute_cost = trailing_mute_cost(frets, formula, string_count);
    let sparse_duplicate_cost = sparse_duplicate_pitch_cost(frets, tuning, formula, string_count);
    let open_bypass_cost = open_chord_tone_bypass_cost(frets, tuning, formula, string_count);
    let fingering_complexity_cost = fingering_complexity_cost(frets, string_count);
    let instrument_cost = instrument_voicing_cost(
        frets,
        tuning.instrument(),
        non_open,
        profile.has_open,
        string_count,
    );
    let instrument_bonus = instrument_voicing_bonus(
        frets,
        tuning.instrument(),
        formula,
        non_open,
        profile.has_open,
        profile.active_count,
        string_count,
    );

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
    score = score.saturating_sub(open_position_bonus(frets, string_count));
    score = score.saturating_sub(open_root_bass_bonus(
        frets,
        tuning,
        bass_rule.preferred(),
        string_count,
    ));
    score = score.saturating_sub(open_bass_grip_bonus(frets, string_count));
    score = score.saturating_sub(closed_shape_bonus(
        frets,
        non_open,
        profile.active_count,
        profile.has_open,
        string_count,
    ));
    score = score.saturating_sub(barre_grip_bonus(frets, non_open, string_count));
    score = score.saturating_sub(compact_low_grip_bonus(frets, non_open, string_count));
    score = score.saturating_sub(jazz_shell_bonus(frets, non_open, formula, string_count));
    let score = score
        + harmonic_defect_cost
        + internal_mute_quality_cost
        + trailing_mute_cost
        + sparse_duplicate_cost
        + open_bypass_cost
        + fingering_complexity_cost
        + instrument_cost;
    score.saturating_sub(instrument_bonus)
}

pub(crate) fn rank_voicing_candidates(
    mut candidates: Vec<VoicingCandidate>,
    options: VoicingOptions,
) -> Vec<VoicingCandidate> {
    candidates.sort_by(compare_voicing_candidates);

    match options.mode {
        VoicingMode::All => candidates,
        VoicingMode::Curated { limit } => rank_diverse_voicing_candidates(candidates, limit),
    }
}

pub(crate) fn rank_diverse_voicing_candidates(
    mut candidates: Vec<VoicingCandidate>,
    limit: usize,
) -> Vec<VoicingCandidate> {
    if limit == 0 {
        return Vec::new();
    }

    let mut selected = Vec::new();
    while selected.len() < limit && !candidates.is_empty() {
        let best_raw_score = candidates
            .first()
            .map(|candidate| candidate.score)
            .unwrap_or(0);
        let candidate_ceiling =
            best_raw_score.saturating_add(diversity_score_window(best_raw_score));
        let best_idx = candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.score <= candidate_ceiling)
            .min_by(|(_, left), (_, right)| {
                effective_voicing_candidate_score(left, &selected)
                    .cmp(&effective_voicing_candidate_score(right, &selected))
                    .then(left.score.cmp(&right.score))
                    .then_with(|| compare_voicing_candidates(left, right))
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        selected.push(candidates.remove(best_idx));
    }

    selected.sort_by(compare_voicing_candidates);
    selected
}

fn compare_voicing_candidates(
    left: &VoicingCandidate,
    right: &VoicingCandidate,
) -> std::cmp::Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| compare_voicing_tie_break(left, right))
        .then_with(|| {
            compare_fingering_compact(
                &left.frets,
                left.string_count,
                &right.frets,
                right.string_count,
            )
        })
}

fn compare_voicing_tie_break(
    left: &VoicingCandidate,
    right: &VoicingCandidate,
) -> std::cmp::Ordering {
    voicing_position_tie_break(&left.frets, left.string_count)
        .cmp(&voicing_position_tie_break(
            &right.frets,
            right.string_count,
        ))
        .then_with(|| {
            right
                .frets
                .iter()
                .take(right.string_count)
                .flatten()
                .count()
                .cmp(&left.frets.iter().take(left.string_count).flatten().count())
        })
}

fn voicing_position_tie_break(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u8 {
    let profile = fret_profile(frets, string_count);
    let max_fret = frets
        .iter()
        .take(string_count)
        .flatten()
        .copied()
        .max()
        .unwrap_or(0);
    let min_non_open = profile.non_open().iter().copied().min().unwrap_or(0);

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

fn compare_fingering_compact(
    left: &[Option<u8>; MAX_STRING_COUNT],
    left_string_count: usize,
    right: &[Option<u8>; MAX_STRING_COUNT],
    right_string_count: usize,
) -> std::cmp::Ordering {
    let (left_key, left_len) = fingering_compact_key(left, left_string_count);
    let (right_key, right_len) = fingering_compact_key(right, right_string_count);
    left_key[..left_len].cmp(&right_key[..right_len])
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

fn effective_voicing_candidate_score(
    candidate: &VoicingCandidate,
    selected: &[VoicingCandidate],
) -> u32 {
    let family = voicing_family(&candidate.frets, candidate.string_count);
    candidate.score
        + u32::try_from(selected_family_count(selected, family)).unwrap_or(0) * 5
        + u32::try_from(selected_position_count(selected, family.position)).unwrap_or(0)
}

fn selected_family_count(selected: &[VoicingCandidate], family: VoicingFamily) -> usize {
    selected
        .iter()
        .filter(|candidate| voicing_family(&candidate.frets, candidate.string_count) == family)
        .count()
}

fn selected_position_count(selected: &[VoicingCandidate], position: PositionFamily) -> usize {
    selected
        .iter()
        .filter(|candidate| {
            voicing_family(&candidate.frets, candidate.string_count).position == position
        })
        .count()
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct VoicingFamily {
    position: PositionFamily,
    string_band: StringBand,
    density: StringDensity,
    internal_mute: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionFamily {
    OpenPosition,
    OpenHigh(PositionBucket),
    Closed(PositionBucket),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PositionBucket {
    Low,
    Middle,
    High,
    Upper,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringBand {
    Full,
    Wide,
    Low,
    Middle,
    High,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StringDensity {
    Full,
    Four,
    Small,
}

fn voicing_family(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> VoicingFamily {
    let mut active_count = 0;
    let mut first = None;
    let mut last = None;
    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        if fret.is_some() {
            active_count += 1;
            first.get_or_insert(idx);
            last = Some(idx);
        }
    }
    let first = first.unwrap_or(0);
    let last = last.unwrap_or(0);
    let profile = fret_profile(frets, string_count);
    let non_open = profile.non_open();
    let has_open = profile.has_open;
    let max_non_open = non_open.iter().copied().max().unwrap_or(0);
    let min_non_open = non_open.iter().copied().min().unwrap_or(0);

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

    let density = if active_count == string_count {
        StringDensity::Full
    } else if active_count + 1 >= string_count && active_count >= 3 {
        StringDensity::Four
    } else {
        StringDensity::Small
    };

    VoicingFamily {
        position,
        string_band,
        density,
        internal_mute: internal_mutes(frets, string_count) > 0,
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

fn position_cost(non_open: &[u8], has_open: bool) -> u32 {
    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    let multiplier = if has_open { 1 } else { 2 };
    u32::from(*min) * multiplier
}

fn relative_fret_cost(non_open: &[u8]) -> u32 {
    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    non_open
        .iter()
        .map(|fret| u32::from(fret.saturating_sub(*min)))
        .sum()
}

fn fret_span_cost(non_open: &[u8]) -> u32 {
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    let span = u32::from(max.saturating_sub(*min));
    span * span
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

fn internal_mute_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    active_count: usize,
    string_count: usize,
) -> u32 {
    let count = internal_mutes(frets, string_count);
    if count == 0 {
        return 0;
    }

    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    let compact_closed = !has_open
        && active_count >= 4
        && fret_span(non_open).is_some_and(|span| span <= 4)
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
    tuning: GuitarTuning,
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
    string_count: usize,
) -> u32 {
    let profile = fret_profile(frets, string_count);
    let non_open = profile.non_open();
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    if *max <= 3 {
        return 0;
    }

    if !profile.has_open {
        return 0;
    }

    if matches!(instrument, Instrument::Ukulele) && *max <= 4 {
        return 0;
    }

    if compact_open_treble_grip(frets, non_open, profile.active_count, string_count) {
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
    let high_position_open_cost = if *min >= 5 { 24 } else { 0 };
    random_high_fret_cost + high_position_open_cost
}

fn compact_open_treble_grip(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    active_count: usize,
    string_count: usize,
) -> bool {
    if active_count + 1 < string_count || trailing_mutes(frets, string_count) > 0 {
        return false;
    }

    let Some(span) = fret_span(non_open) else {
        return false;
    };
    let Some(min) = non_open.iter().min() else {
        return false;
    };
    let Some(max) = non_open.iter().max() else {
        return false;
    };
    if span > 4 || *min > 5 || *max > 7 {
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
    tuning: GuitarTuning,
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

fn voicing_omission_cost(omissions: &[String], formula: &ChordFormula) -> u32 {
    let upper_structure = has_upper_structure(formula);
    omissions
        .iter()
        .map(|omission| match omission.as_str() {
            "1" => {
                if upper_structure {
                    45
                } else {
                    140
                }
            }
            "3" => 160,
            "5" => {
                if upper_structure {
                    0
                } else {
                    90
                }
            }
            other => {
                debug_assert!(false, "unexpected inferred omission degree '{other}'");
                100
            }
        })
        .sum()
}

fn internal_mute_quality_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    formula: &ChordFormula,
    instrument: Instrument,
    string_count: usize,
) -> u32 {
    let count = internal_mutes(frets, string_count);
    if count == 0 {
        return 0;
    }

    let active_count = frets.iter().take(string_count).flatten().count();
    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    let compact_extended_shell = has_upper_structure(formula)
        && !has_open
        && active_count <= 4
        && fret_span(non_open).is_some_and(|span| span <= 4);
    if compact_extended_shell {
        return 0;
    }

    if !has_upper_structure(formula) && active_count >= 5 {
        return count * 36;
    }

    if has_open {
        if matches!(instrument, Instrument::Ukulele) {
            count * 12
        } else {
            count * 24
        }
    } else {
        count * 20
    }
}

fn trailing_mute_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    formula: &ChordFormula,
    string_count: usize,
) -> u32 {
    let trailing = trailing_mutes(frets, string_count);
    if trailing == 0 {
        return 0;
    }

    let active_count = frets.iter().take(string_count).flatten().count();
    let compact_extended_shell = has_upper_structure(formula) && active_count <= 4;
    let unit = if compact_extended_shell {
        2
    } else if active_count >= 5 {
        8
    } else {
        10
    };

    u32::try_from(trailing).unwrap_or(0) * unit
}

fn instrument_voicing_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    instrument: Instrument,
    non_open: &[u8],
    has_open: bool,
    string_count: usize,
) -> u32 {
    if !matches!(instrument, Instrument::Ukulele) {
        return 0;
    }

    ukulele_high_position_cost(non_open, has_open) + ukulele_treble_mute_cost(frets, string_count)
}

fn instrument_voicing_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    instrument: Instrument,
    formula: &ChordFormula,
    non_open: &[u8],
    has_open: bool,
    active_count: usize,
    string_count: usize,
) -> u32 {
    match instrument {
        Instrument::Guitar => {
            guitar_open_position_bonus(frets, non_open, has_open, active_count, string_count)
                + guitar_open_treble_extension_bonus(
                    frets,
                    formula,
                    non_open,
                    has_open,
                    active_count,
                    string_count,
                )
        }
        Instrument::Ukulele => {
            ukulele_diagonal_open_grip_bonus(non_open, has_open, active_count, string_count)
        }
        Instrument::Guitar7 | Instrument::Guitar8 => 0,
    }
}

fn guitar_open_position_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    has_open: bool,
    active_count: usize,
    string_count: usize,
) -> u32 {
    if string_count != 6 || !has_open || active_count < 4 {
        return 0;
    }

    let Some(max) = non_open.iter().max() else {
        return 0;
    };
    if *max <= 3 && trailing_mutes(frets, string_count) == 0 {
        8
    } else {
        0
    }
}

fn guitar_open_treble_extension_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    formula: &ChordFormula,
    non_open: &[u8],
    has_open: bool,
    active_count: usize,
    string_count: usize,
) -> u32 {
    if string_count != 6
        || !has_open
        || active_count < 5
        || frets[string_count - 1] != Some(0)
        || !formula_has_degree(formula, 9)
        || formula_has_degree(formula, 7)
    {
        return 0;
    }

    let Some(span) = fret_span(non_open) else {
        return 0;
    };
    let Some(max) = non_open.iter().max() else {
        return 0;
    };
    if span <= 4 && *max <= 5 { 24 } else { 0 }
}

fn ukulele_diagonal_open_grip_bonus(
    non_open: &[u8],
    has_open: bool,
    active_count: usize,
    string_count: usize,
) -> u32 {
    if string_count != 4 || !has_open || active_count != string_count || non_open.len() < 3 {
        return 0;
    }

    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    if *max > 4 || max.saturating_sub(*min) + 1 != u8::try_from(non_open.len()).unwrap_or(0) {
        return 0;
    }

    let mut seen = [false; 5];
    for fret in non_open {
        let offset = usize::from(fret.saturating_sub(*min));
        if offset >= seen.len() || seen[offset] {
            return 0;
        }
        seen[offset] = true;
    }

    12
}

fn ukulele_high_position_cost(non_open: &[u8], has_open: bool) -> u32 {
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };

    if *min >= 7 {
        let excess = u32::from(min.saturating_sub(6));
        return 36 + excess * excess * 3;
    }

    if *max >= 7 && *min <= 4 {
        let excess = u32::from(max.saturating_sub(6));
        return if has_open {
            24 + excess * 4
        } else {
            18 + excess * 4
        };
    }

    0
}

fn ukulele_treble_mute_cost(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    if string_count != 4 || frets[string_count - 1].is_some() {
        return 0;
    }

    let active_count = frets.iter().take(string_count).flatten().count();
    if active_count + 1 == string_count {
        18
    } else {
        8
    }
}

fn sparse_duplicate_pitch_cost(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    tuning: GuitarTuning,
    formula: &ChordFormula,
    string_count: usize,
) -> u32 {
    let active_count = frets.iter().take(string_count).flatten().count();
    let internal_mute_count = internal_mutes(frets, string_count);
    if active_count >= 5 && internal_mute_count == 0 {
        return 0;
    }

    let open_complete_grip = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0)
        && internal_mute_count == 0;
    if active_count == string_count
        && open_complete_grip
        && trailing_mutes(frets, string_count) == 0
    {
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
    tuning: GuitarTuning,
    formula: &ChordFormula,
    string_count: usize,
) -> u32 {
    let profile = fret_profile(frets, string_count);
    if !profile.has_open
        || dense_open_bass_grip(frets, string_count)
        || compact_open_treble_grip(
            frets,
            profile.non_open(),
            profile.active_count,
            string_count,
        )
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
        .filter(|(string, _)| target.contains(tuning.note(*string).pitch_class()))
        .map(|(_, fret)| {
            let excess = u32::from(fret.saturating_sub(3));
            excess * excess * 12
        })
        .sum()
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
    let mut barre_count = 0;

    while let Some(barre) = best_barre(frets, &covered, string_count) {
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
}

fn best_barre(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    covered: &[bool; MAX_STRING_COUNT],
    string_count: usize,
) -> Option<BarreCandidate> {
    let mut best = None;

    for fret in 1..=MAX_STANDARD_FRET {
        for start in 0..string_count {
            if frets[start] != Some(fret) || covered[start] {
                continue;
            }
            for end in (start + 1)..string_count {
                if frets[end] != Some(fret) || covered[end] {
                    continue;
                }
                if !barre_range_valid(frets, fret, start, end) {
                    continue;
                }

                let exact_count = (start..=end)
                    .filter(|string| frets[*string] == Some(fret) && !covered[*string])
                    .count();
                if exact_count < 2 {
                    continue;
                }

                let candidate = BarreCandidate {
                    fret,
                    start,
                    end,
                    saving: exact_count - 1,
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

fn barre_range_valid(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    fret: u8,
    start: usize,
    end: usize,
) -> bool {
    (start..=end).all(|string| frets[string].is_some_and(|value| value >= fret))
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

fn open_position_bonus(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let active_count = frets.iter().take(string_count).flatten().count();
    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    let max_fret = frets
        .iter()
        .take(string_count)
        .flatten()
        .copied()
        .max()
        .unwrap_or(0);
    let min_active = if string_count >= 6 {
        4
    } else {
        string_count.saturating_sub(1)
    };
    if has_open && max_fret <= 3 && active_count >= min_active {
        if active_count == string_count {
            22
        } else if active_count + 1 == string_count {
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
    tuning: GuitarTuning,
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

fn open_bass_grip_bonus(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let mut has_open = false;
    let mut first_fretted = None;
    let mut last_fretted = None;
    let mut fretted_count = 0;
    let mut min_non_open = None::<u8>;
    let mut max_non_open = None::<u8>;

    for (idx, fret) in frets.iter().take(string_count).enumerate() {
        match fret {
            Some(0) => {
                if first_fretted.is_some() {
                    return 0;
                }
                has_open = true;
            }
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

fn closed_shape_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    active_count: usize,
    has_open: bool,
    string_count: usize,
) -> u32 {
    let min_active = if string_count >= 6 {
        4
    } else {
        string_count.saturating_sub(1)
    };
    if has_open || active_count < min_active || internal_mutes(frets, string_count) > 0 {
        return 0;
    }

    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return 0;
    };
    let min_count = non_open.iter().filter(|fret| *fret == min).count();
    if min_count < 2 {
        return 0;
    }

    if max.saturating_sub(*min) <= 2 {
        let base = match *min {
            0..=2 => 30,
            3..=5 => 6,
            6..=8 => 12,
            _ => 8,
        };
        if active_count == string_count {
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
    string_count: usize,
) -> u32 {
    let active_count = frets.iter().take(string_count).flatten().count();
    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    if has_open || active_count != string_count || internal_mutes(frets, string_count) > 0 {
        return 0;
    }

    let Some(span) = fret_span(non_open) else {
        return 0;
    };
    if span > 4 {
        return 0;
    }

    let Some(min) = non_open.iter().min() else {
        return 0;
    };
    let min_count = non_open.iter().filter(|fret| *fret == min).count();
    if min_count < 2 {
        return 0;
    }

    let reaches_treble = frets[string_count - 1].is_some();
    if !reaches_treble {
        return 0;
    }

    match *min {
        0..=5 => 12,
        6..=8 => 4,
        _ => 2,
    }
}

fn compact_low_grip_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    string_count: usize,
) -> u32 {
    let active_count = frets.iter().take(string_count).flatten().count();
    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    if has_open || active_count + 1 < string_count {
        return 0;
    }

    let Some(span) = fret_span(non_open) else {
        return 0;
    };
    let min = non_open.iter().copied().min().unwrap_or(0);
    if min <= 3 && span <= 2 { 8 } else { 0 }
}

fn jazz_shell_bonus(
    frets: &[Option<u8>; MAX_STRING_COUNT],
    non_open: &[u8],
    formula: &ChordFormula,
    string_count: usize,
) -> u32 {
    if !has_upper_structure(formula) {
        return 0;
    }

    let active_count = frets.iter().take(string_count).flatten().count();
    let has_open = frets
        .iter()
        .take(string_count)
        .flatten()
        .any(|fret| *fret == 0);
    if has_open || !(3..=4).contains(&active_count) {
        return 0;
    }

    let Some(span) = fret_span(non_open) else {
        return 0;
    };
    if span > 4 {
        return 0;
    }

    let muted = internal_mutes(frets, string_count);
    if muted > 1 {
        return 0;
    }

    match active_count {
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

fn fret_span(non_open: &[u8]) -> Option<u8> {
    let (Some(min), Some(max)) = (non_open.iter().min(), non_open.iter().max()) else {
        return None;
    };
    Some(max.saturating_sub(*min))
}

fn trailing_mutes(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> usize {
    let Some(last_played) = frets
        .iter()
        .take(string_count)
        .enumerate()
        .filter_map(|(idx, fret)| fret.map(|_| idx))
        .next_back()
    else {
        return 0;
    };

    string_count - 1 - last_played
}

fn internal_mutes(frets: &[Option<u8>; MAX_STRING_COUNT], string_count: usize) -> u32 {
    let active = &frets[..string_count];
    let first = active.iter().position(Option::is_some);
    let last = active.iter().rposition(Option::is_some);
    let (Some(first), Some(last)) = (first, last) else {
        return 0;
    };
    let count = (first..=last).filter(|idx| frets[*idx].is_none()).count();
    u32::try_from(count).unwrap_or(0)
}
