use crate::candidate_record::{CandidateFormulaSummary, CandidateRecord};
use crate::formula::{
    ChordFormula, has_omitted_alteration, has_redundant_alteration, raw_tones_from_spec,
};
use crate::inline_vec::InlineVec;
use crate::notes::{NoteLetter, NoteName, PitchClass, PitchSet};
use crate::parse::{push_unique, validate_descriptor};
use crate::symbol::{Alteration, ChordSpec, Extension, MAX_ALTERATIONS, Quality, Seventh};

pub(crate) const PITCH_SET_KEY_COUNT: usize = 1 << 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CandidateIndexData {
    pub(crate) exact_offsets: [usize; PITCH_SET_KEY_COUNT + 1],
    pub(crate) exact_indices: Vec<usize>,
    pub(crate) superset_offsets: [usize; PITCH_SET_KEY_COUNT + 1],
    pub(crate) superset_indices: Vec<usize>,
}

pub(crate) fn build_candidate_records() -> Vec<CandidateRecord> {
    let mut records = Vec::new();
    for spec in candidate_specs() {
        let raw_tones = raw_tones_from_spec(&spec);
        for root in candidate_root_spellings() {
            let formula = ChordFormula::from_raw_parts(root, &raw_tones);
            if formula.has_duplicate_pitch_classes() {
                continue;
            }
            if formula.has_spelling_outside_double_accidentals() {
                continue;
            }
            records.push(CandidateRecord {
                root,
                spec: spec.clone(),
                summary: formula_summary(&formula),
                formula_set: formula.pitch_set(),
            });
        }
    }
    records
}

fn formula_summary(formula: &ChordFormula) -> CandidateFormulaSummary {
    let mut summary = CandidateFormulaSummary {
        tone_count: formula.tones().len() as u8,
        degree_by_pitch: [0; 12],
        semitone_by_pitch: [0; 12],
    };
    for tone in formula.tones() {
        let idx = usize::from(tone.pitch_class);
        summary.degree_by_pitch[idx] = tone.degree;
        summary.semitone_by_pitch[idx] = tone.semitones as i8;
    }
    summary
}

pub(crate) fn build_candidate_indices(records: &[CandidateRecord]) -> CandidateIndexData {
    let mut exact_counts = [0usize; PITCH_SET_KEY_COUNT];
    let mut superset_counts = [0usize; PITCH_SET_KEY_COUNT];

    for record in records {
        exact_counts[usize::from(record.formula_set.bits)] += 1;
        for_each_subset(record.formula_set, |subset| {
            superset_counts[usize::from(subset.bits)] += 1;
        });
    }

    let exact_offsets = prefix_offsets(&exact_counts);
    let superset_offsets = prefix_offsets(&superset_counts);
    let mut exact_indices = vec![0usize; exact_offsets[PITCH_SET_KEY_COUNT]];
    let mut superset_indices = vec![0usize; superset_offsets[PITCH_SET_KEY_COUNT]];
    let mut exact_write_offsets = exact_offsets;
    let mut superset_write_offsets = superset_offsets;

    for (idx, record) in records.iter().enumerate() {
        let exact_key = usize::from(record.formula_set.bits);
        let exact_write_idx = exact_write_offsets[exact_key];
        exact_indices[exact_write_idx] = idx;
        exact_write_offsets[exact_key] += 1;

        for_each_subset(record.formula_set, |subset| {
            let key = usize::from(subset.bits);
            let write_idx = superset_write_offsets[key];
            superset_indices[write_idx] = idx;
            superset_write_offsets[key] += 1;
        });
    }

    CandidateIndexData {
        exact_offsets,
        exact_indices,
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

fn candidate_root_spellings() -> Vec<NoteName> {
    let mut roots = Vec::new();
    for pitch in 0..12 {
        push_candidate_spellings(&mut roots, PitchClass(pitch));
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

fn prefer_flat_pitch(pitch: PitchClass) -> bool {
    matches!(pitch.value(), 1 | 3 | 8 | 10)
}

fn candidate_specs() -> Vec<ChordSpec> {
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
    let mut options = vec![InlineVec::default()];
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
    let extension_degree = extension.map(crate::parse::extension_degree);
    for alteration in alterations {
        if Some(alteration.degree) == extension_degree {
            continue;
        }

        let current = options.clone();
        for mut option in current {
            let has_same_degree = option
                .iter()
                .any(|existing: &Alteration| existing.degree == alteration.degree);
            if has_same_degree {
                continue;
            }
            let _ = option.push(alteration);
            option.sort_unstable();
            let blocked = options.iter().any(|existing| existing == &option);
            if !blocked {
                options.push(option);
            }
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
