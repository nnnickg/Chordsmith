use std::borrow::Cow;

use crate::formula::raw_tones_without_omissions;
use crate::inline_vec::InlineVec;
use crate::notes::{NoteLetter, NoteName};
use crate::symbol::{
    Alteration, ChordSpec, Extension, Quality, Seventh, alteration_prefix, is_supported_alteration,
};
use crate::{ChordsmithError, MAX_NOTE_ACCIDENTALS};

pub(crate) fn normalize_chart_glyphs(input: &str) -> Cow<'_, str> {
    if !input
        .chars()
        .any(|ch| matches!(ch, '♭' | '♯' | '𝄫' | '𝄪' | '−' | '–'))
    {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '♭' => out.push('b'),
            '♯' => out.push('#'),
            '𝄫' => out.push_str("bb"),
            '𝄪' => out.push_str("##"),
            '−' | '–' => out.push('-'),
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

pub(crate) fn parse_note_prefix(input: &str) -> Result<(NoteName, &str), ChordsmithError> {
    let mut chars = input.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(ChordsmithError::new("empty note"));
    };
    let Some(letter) = NoteLetter::from_ascii(first) else {
        return Err(ChordsmithError::new(format!("invalid note root '{first}'")));
    };

    let mut accidental = 0i8;
    let mut accidental_kind = None;
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        match ch {
            '#' | 'b' => {
                if accidental_kind.is_some_and(|kind| kind != ch) {
                    break;
                }
                accidental_kind = Some(ch);
                let step = if ch == '#' { 1 } else { -1 };
                let Some(next) = accidental.checked_add(step) else {
                    return Err(ChordsmithError::new(format!(
                        "too many accidentals in note '{input}'"
                    )));
                };
                if next.unsigned_abs() > MAX_NOTE_ACCIDENTALS {
                    return Err(ChordsmithError::new(format!(
                        "too many accidentals in note '{input}'"
                    )));
                }
                accidental = next;
                end = idx + ch.len_utf8();
            }
            _ => break,
        }
    }

    Ok((NoteName { letter, accidental }, &input[end..]))
}

pub(crate) fn split_bass(rest: &str) -> Result<(&str, Option<NoteName>), ChordsmithError> {
    let mut bass_idx = None;
    for (idx, ch) in rest.char_indices() {
        if ch != '/' || is_six_nine_slash(rest, idx) {
            continue;
        }
        if bass_idx.replace(idx).is_some() {
            return Err(ChordsmithError::new(format!(
                "invalid slash chord syntax '{rest}'"
            )));
        }
    }

    let Some(idx) = bass_idx else {
        return Ok((rest, None));
    };
    let after = &rest[idx + 1..];
    if after.is_empty() {
        return Err(ChordsmithError::new("slash chord is missing bass note"));
    }
    let Some(first) = after.chars().next() else {
        return Err(ChordsmithError::new("slash chord is missing bass note"));
    };
    if NoteLetter::from_ascii(first).is_none() {
        return Err(ChordsmithError::new(format!(
            "invalid slash chord bass '{after}'"
        )));
    }
    let bass = NoteName::parse(after)?;
    Ok((&rest[..idx], Some(bass)))
}

fn is_six_nine_slash(input: &str, slash_idx: usize) -> bool {
    let bytes = input.as_bytes();
    slash_idx > 0
        && slash_idx + 1 < bytes.len()
        && bytes[slash_idx - 1] == b'6'
        && bytes[slash_idx + 1] == b'9'
}

pub(crate) fn parse_descriptor(input: &str) -> Result<ChordSpec, ChordsmithError> {
    let text = normalize_descriptor_text(input)?;

    let mut spec = ChordSpec::default();
    let mut explicit_quality = false;
    let mut half_diminished_can_extend = false;
    let mut rest = text.as_ref();

    if !starts_with_major_extension(rest)
        && let Some(next) = take_prefix(rest, &["min", "Min", "MIN", "m", "-"])
    {
        set_quality(&mut spec, &mut explicit_quality, Quality::Minor, "m")?;
        rest = next;
    } else if !starts_with_major_extension(rest)
        && let Some(next) = take_prefix(rest, &["maj", "Maj", "MAJ", "M"])
    {
        set_quality(&mut spec, &mut explicit_quality, Quality::Major, "maj")?;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["dim", "Dim", "DIM", "o", "°"]) {
        set_quality(&mut spec, &mut explicit_quality, Quality::Diminished, "dim")?;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["aug", "Aug", "AUG", "+"]) {
        set_quality(&mut spec, &mut explicit_quality, Quality::Augmented, "aug")?;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["sus2", "Sus2", "SUS2"]) {
        set_quality(&mut spec, &mut explicit_quality, Quality::Sus2, "sus2")?;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["sus4", "Sus4", "SUS4", "sus", "Sus", "SUS"]) {
        set_quality(&mut spec, &mut explicit_quality, Quality::Sus4, "sus4")?;
        rest = next;
    } else if let Some(next) = take_prefix(rest, &["5"]) {
        set_quality(&mut spec, &mut explicit_quality, Quality::Power, "5")?;
        rest = next;
    }

    while !rest.is_empty() {
        if let Some(next) = take_prefix(rest, &["ø7", "Ø7"]) {
            set_half_diminished(&mut spec, &mut explicit_quality, "ø7")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["ø", "Ø"]) {
            set_half_diminished(&mut spec, &mut explicit_quality, "ø")?;
            half_diminished_can_extend = true;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["alt", "Alt", "ALT"]) {
            if spec.alt {
                return Err(duplicate_descriptor("alt"));
            }
            spec.alt = true;
            rest = next;
        } else if let Some(next) =
            take_prefix(rest, &["maj13", "Maj13", "MAJ13", "M13", "Δ13", "△13"])
        {
            set_extension(&mut spec, Extension::Thirteenth, Seventh::Major, "maj13")?;
            rest = next;
        } else if let Some(next) =
            take_prefix(rest, &["maj11", "Maj11", "MAJ11", "M11", "Δ11", "△11"])
        {
            set_extension(&mut spec, Extension::Eleventh, Seventh::Major, "maj11")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["maj9", "Maj9", "MAJ9", "M9", "Δ9", "△9"])
        {
            set_extension(&mut spec, Extension::Ninth, Seventh::Major, "maj9")?;
            rest = next;
        } else if let Some(next) =
            take_prefix(rest, &["maj7", "Maj7", "MAJ7", "M7", "Δ7", "△7", "Δ", "△"])
        {
            set_seventh(&mut spec, Seventh::Major, "maj7")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["sus2", "Sus2", "SUS2"]) {
            set_quality(&mut spec, &mut explicit_quality, Quality::Sus2, "sus2")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["sus4", "Sus4", "SUS4", "sus", "Sus", "SUS"])
        {
            set_quality(&mut spec, &mut explicit_quality, Quality::Sus4, "sus4")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add13"]) {
            push_unique_descriptor(&mut spec.adds, 13, "add13")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add11", "add4"]) {
            push_unique_descriptor(&mut spec.adds, 11, "add11")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["add9", "add2"]) {
            push_unique_descriptor(&mut spec.adds, 9, "add9")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit5", "no5"]) {
            push_unique_descriptor(&mut spec.omissions, 5, "no5")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit3", "no3"]) {
            push_unique_descriptor(&mut spec.omissions, 3, "no3")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["omit1", "no1"]) {
            push_unique_descriptor(&mut spec.omissions, 1, "no1")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["13"]) {
            let seventh = default_seventh_for_extension(spec.quality);
            set_extension_or_half_diminished_extension(
                &mut spec,
                &mut half_diminished_can_extend,
                Extension::Thirteenth,
                seventh,
                "13",
            )?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["11"]) {
            let seventh = default_seventh_for_extension(spec.quality);
            set_extension_or_half_diminished_extension(
                &mut spec,
                &mut half_diminished_can_extend,
                Extension::Eleventh,
                seventh,
                "11",
            )?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["9"]) {
            let seventh = default_seventh_for_extension(spec.quality);
            set_extension_or_half_diminished_extension(
                &mut spec,
                &mut half_diminished_can_extend,
                Extension::Ninth,
                seventh,
                "9",
            )?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["6/9", "69"]) {
            set_six_nine(&mut spec)?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["7"]) {
            let seventh = if spec.quality == Quality::Diminished {
                Seventh::Diminished
            } else {
                Seventh::Minor
            };
            set_seventh(&mut spec, seventh, "7")?;
            rest = next;
        } else if let Some(next) = take_prefix(rest, &["6"]) {
            set_sixth(&mut spec)?;
            rest = next;
        } else if let Some((alteration, next)) = take_alteration(rest) {
            if spec.alterations.contains(&alteration) {
                return Err(duplicate_descriptor(&format!(
                    "{}{}",
                    alteration_prefix(alteration.accidental),
                    alteration.degree
                )));
            }
            upsert_alteration(&mut spec.alterations, alteration);
            rest = next;
        } else {
            return Err(ChordsmithError::new(format!(
                "unknown chord descriptor near '{rest}'"
            )));
        }
    }

    validate_descriptor(&spec)?;
    normalize_spec(&mut spec);
    Ok(spec)
}

fn normalize_descriptor_text(input: &str) -> Result<Cow<'_, str>, ChordsmithError> {
    if !input
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '(' || ch == ')')
    {
        return Ok(Cow::Borrowed(input));
    }

    let mut out = String::new();
    let mut chars = input.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch.is_whitespace() {
            return Err(ChordsmithError::new(format!(
                "invalid chord descriptor: whitespace inside '{input}'"
            )));
        }
        match ch {
            '(' => {
                let start = idx + ch.len_utf8();
                let Some((end, _)) = chars.by_ref().find(|(_, inner)| *inner == ')') else {
                    return Err(ChordsmithError::new(format!(
                        "invalid chord descriptor: unclosed '(' in '{input}'"
                    )));
                };
                let group = &input[start..end];
                if group.is_empty()
                    || group
                        .chars()
                        .any(|inner| inner.is_whitespace() || inner == '(' || inner == ')')
                    || !parenthesized_descriptor_group_is_valid(group)
                {
                    return Err(ChordsmithError::new(format!(
                        "invalid chord descriptor group '({group})'"
                    )));
                }
                out.push_str(group);
            }
            ')' => {
                return Err(ChordsmithError::new(format!(
                    "invalid chord descriptor: unmatched ')' in '{input}'"
                )));
            }
            _ => out.push(ch),
        }
    }

    Ok(Cow::Owned(out))
}

fn parenthesized_descriptor_group_is_valid(mut input: &str) -> bool {
    while !input.is_empty() {
        let Some(next) = take_descriptor_token(input) else {
            return false;
        };
        if next.len() == input.len() {
            return false;
        }
        input = next;
    }
    true
}

fn take_descriptor_token(input: &str) -> Option<&str> {
    if let Some(next) = take_prefix(
        input,
        &[
            "maj13", "Maj13", "MAJ13", "M13", "Δ13", "△13", "maj11", "Maj11", "MAJ11", "M11",
            "Δ11", "△11", "maj9", "Maj9", "MAJ9", "M9", "Δ9", "△9", "maj7", "Maj7", "MAJ7", "M7",
            "Δ7", "△7", "add13", "add11", "add4", "add9", "add2", "omit5", "omit3", "omit1", "no5",
            "no3", "no1", "sus2", "Sus2", "SUS2", "sus4", "Sus4", "SUS4", "sus", "Sus", "SUS",
            "alt", "Alt", "ALT", "ø7", "Ø7", "ø", "Ø", "13", "11", "9", "6/9", "69", "7", "6", "Δ",
            "△",
        ],
    ) {
        return Some(next);
    }
    take_alteration(input).map(|(_, next)| next)
}

fn duplicate_descriptor(token: &str) -> ChordsmithError {
    ChordsmithError::new(format!(
        "invalid chord descriptor: duplicate or conflicting '{token}'"
    ))
}

fn reject_numbered_harmony(spec: &ChordSpec, token: &str) -> Result<(), ChordsmithError> {
    if spec.alt || spec.sixth || spec.seventh != Seventh::None || spec.extension.is_some() {
        Err(duplicate_descriptor(token))
    } else {
        Ok(())
    }
}

fn set_extension(
    spec: &mut ChordSpec,
    extension: Extension,
    seventh: Seventh,
    token: &str,
) -> Result<(), ChordsmithError> {
    reject_numbered_harmony(spec, token)?;
    spec.seventh = seventh;
    spec.extension = Some(extension);
    Ok(())
}

fn set_extension_or_half_diminished_extension(
    spec: &mut ChordSpec,
    half_diminished_can_extend: &mut bool,
    extension: Extension,
    seventh: Seventh,
    token: &str,
) -> Result<(), ChordsmithError> {
    if *half_diminished_can_extend && is_half_diminished_shorthand_base(spec) {
        spec.extension = Some(extension);
        *half_diminished_can_extend = false;
        return Ok(());
    }

    set_extension(spec, extension, seventh, token)
}

fn set_seventh(spec: &mut ChordSpec, seventh: Seventh, token: &str) -> Result<(), ChordsmithError> {
    reject_numbered_harmony(spec, token)?;
    spec.seventh = seventh;
    Ok(())
}

fn set_sixth(spec: &mut ChordSpec) -> Result<(), ChordsmithError> {
    reject_numbered_harmony(spec, "6")?;
    spec.sixth = true;
    Ok(())
}

fn set_six_nine(spec: &mut ChordSpec) -> Result<(), ChordsmithError> {
    reject_numbered_harmony(spec, "6/9")?;
    spec.sixth = true;
    push_unique_descriptor(&mut spec.adds, 9, "add9")?;
    Ok(())
}

pub(crate) fn push_unique_descriptor<const N: usize>(
    values: &mut InlineVec<u8, N>,
    value: u8,
    token: &str,
) -> Result<(), ChordsmithError> {
    if values.contains(&value) {
        return Err(duplicate_descriptor(token));
    }
    push_unique(values, value);
    Ok(())
}

fn set_quality(
    spec: &mut ChordSpec,
    explicit_quality: &mut bool,
    quality: Quality,
    token: &str,
) -> Result<(), ChordsmithError> {
    if *explicit_quality {
        return Err(duplicate_descriptor(token));
    }
    spec.quality = quality;
    *explicit_quality = true;
    Ok(())
}

fn set_half_diminished(
    spec: &mut ChordSpec,
    explicit_quality: &mut bool,
    token: &str,
) -> Result<(), ChordsmithError> {
    if *explicit_quality
        || spec.seventh != Seventh::None
        || spec.extension.is_some()
        || spec.sixth
        || spec.alt
        || !spec.adds.is_empty()
        || !spec.alterations.is_empty()
        || !spec.omissions.is_empty()
    {
        return Err(duplicate_descriptor(token));
    }

    spec.quality = Quality::Minor;
    spec.seventh = Seventh::Minor;
    upsert_alteration(
        &mut spec.alterations,
        Alteration {
            degree: 5,
            accidental: -1,
        },
    );
    *explicit_quality = true;
    Ok(())
}

fn is_half_diminished_shorthand_base(spec: &ChordSpec) -> bool {
    spec.quality == Quality::Minor
        && spec.seventh == Seventh::Minor
        && spec.extension.is_none()
        && !spec.sixth
        && !spec.alt
        && spec.adds.is_empty()
        && spec.omissions.is_empty()
        && spec.alterations.as_slice()
            == [Alteration {
                degree: 5,
                accidental: -1,
            }]
}

pub(crate) fn validate_descriptor(spec: &ChordSpec) -> Result<(), ChordsmithError> {
    if spec.quality == Quality::Power
        && (spec.seventh != Seventh::None
            || spec.extension.is_some()
            || spec.sixth
            || spec.alt
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
    {
        return Err(ChordsmithError::new(
            "invalid chord descriptor: power chords cannot be combined with other chord tones",
        ));
    }

    if spec.alt
        && (spec.quality != Quality::Major
            || spec.seventh == Seventh::Major
            || spec.extension.is_some()
            || spec.sixth
            || !spec.adds.is_empty()
            || !spec.alterations.is_empty()
            || !spec.omissions.is_empty())
    {
        return Err(ChordsmithError::new(
            "invalid chord descriptor: alt is already a complete altered dominant set",
        ));
    }

    if let Some(alteration) = spec
        .alterations
        .iter()
        .find(|alteration| !is_supported_alteration(alteration.degree, alteration.accidental))
    {
        return Err(ChordsmithError::new(format!(
            "invalid chord descriptor: unsupported alteration '{}{}'",
            alteration_prefix(alteration.accidental),
            alteration.degree
        )));
    }

    if let Some(extension) = spec.extension {
        let degree = extension_degree(extension);
        if spec
            .alterations
            .iter()
            .any(|alteration| alteration.degree == degree)
        {
            return Err(ChordsmithError::new(format!(
                "invalid chord descriptor: extension {degree} cannot also be altered"
            )));
        }
    }

    if let Some(alteration) = spec
        .alterations
        .iter()
        .find(|alteration| spec.adds.contains(&alteration.degree))
    {
        return Err(ChordsmithError::new(format!(
            "invalid chord descriptor: added degree {} cannot also be altered",
            alteration.degree
        )));
    }

    validate_explicit_omissions(spec)?;

    Ok(())
}

fn validate_explicit_omissions(spec: &ChordSpec) -> Result<(), ChordsmithError> {
    if spec.omissions.is_empty() {
        return Ok(());
    }

    let raw = raw_tones_without_omissions(spec);
    for omission in &spec.omissions {
        if !raw.iter().any(|tone| tone.degree == *omission) {
            return Err(ChordsmithError::new(format!(
                "invalid chord descriptor: cannot omit absent degree {omission}"
            )));
        }
    }

    let remaining = raw
        .iter()
        .filter(|tone| !spec.omissions.contains(&tone.degree))
        .count();
    if remaining < 2 {
        return Err(ChordsmithError::new(
            "invalid chord descriptor: omissions remove too much harmonic content",
        ));
    }

    Ok(())
}

pub(crate) fn extension_degree(extension: Extension) -> u8 {
    match extension {
        Extension::Ninth => 9,
        Extension::Eleventh => 11,
        Extension::Thirteenth => 13,
    }
}

pub(crate) fn take_prefix<'a>(input: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes
        .iter()
        .find_map(|prefix| input.strip_prefix(prefix))
}

fn starts_with_major_extension(input: &str) -> bool {
    [
        "maj13", "Maj13", "MAJ13", "M13", "Δ13", "△13", "maj11", "Maj11", "MAJ11", "M11", "Δ11",
        "△11", "maj9", "Maj9", "MAJ9", "M9", "Δ9", "△9", "maj7", "Maj7", "MAJ7", "M7", "Δ7", "△7",
        "Δ", "△",
    ]
    .iter()
    .any(|prefix| input.starts_with(prefix))
}

pub(crate) fn take_alteration(input: &str) -> Option<(Alteration, &str)> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    let step: i8 = match first {
        'b' => -1,
        '#' | '+' => 1,
        _ => return None,
    };
    let mut accidental = step;
    let mut end = first.len_utf8();
    if matches!(first, 'b' | '#') {
        for (idx, ch) in chars {
            if ch != first {
                break;
            }
            let next = accidental + step;
            if next.unsigned_abs() > MAX_NOTE_ACCIDENTALS {
                break;
            }
            accidental = next;
            end = idx + ch.len_utf8();
        }
    }
    let rest = &input[end..];

    for degree in [13u8, 11, 9, 5] {
        let text = degree.to_string();
        if let Some(next) = rest.strip_prefix(&text) {
            return Some((Alteration { degree, accidental }, next));
        }
    }
    None
}

fn default_seventh_for_extension(quality: Quality) -> Seventh {
    if quality == Quality::Diminished {
        Seventh::Diminished
    } else {
        Seventh::Minor
    }
}

pub(crate) fn push_unique<const N: usize>(values: &mut InlineVec<u8, N>, value: u8) {
    if !values.contains(&value) {
        let _ = values.push(value);
        values.sort_unstable();
    }
}

pub(crate) fn upsert_alteration<const N: usize>(
    values: &mut InlineVec<Alteration, N>,
    value: Alteration,
) {
    if values.contains(&value) {
        return;
    }

    let _ = values.push(value);
    values.sort_unstable();
}

pub(crate) fn normalize_spec(spec: &mut ChordSpec) {
    if spec.quality == Quality::Power {
        spec.seventh = Seventh::None;
        spec.extension = None;
        spec.sixth = false;
        spec.alt = false;
        spec.adds.clear();
        spec.alterations.clear();
        spec.omissions.clear();
        return;
    }

    if spec.alt {
        spec.quality = Quality::Major;
        spec.seventh = Seventh::Minor;
        return;
    }

    if spec.extension.is_some() {
        spec.sixth = false;
    }
}
