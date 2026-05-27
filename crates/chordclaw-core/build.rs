#![allow(dead_code, unused_imports)]
#![allow(clippy::panic)]

use std::env;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChordClawErrorKind {
    Data,
    Usage,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChordClawError {
    kind: ChordClawErrorKind,
    message: String,
}

impl ChordClawError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self::with_kind(ChordClawErrorKind::Data, message)
    }

    pub fn with_kind(kind: ChordClawErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> ChordClawErrorKind {
        self.kind
    }
}

impl fmt::Display for ChordClawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ChordClawError {}

pub const GUITAR_STRING_COUNT: usize = 6;
pub const GUITAR7_STRING_COUNT: usize = 7;
pub const GUITAR8_STRING_COUNT: usize = 8;
pub const UKULELE_STRING_COUNT: usize = 4;
pub(crate) const MAX_STRING_COUNT: usize = GUITAR8_STRING_COUNT;
pub const MAX_STANDARD_FRET: u8 = 30;
pub(crate) const MAX_NOTE_ACCIDENTALS: u8 = 2;

#[path = "src/candidate_data_builder.rs"]
mod candidate_data_builder;
#[path = "src/candidate_record.rs"]
mod candidate_record;
#[path = "src/formula.rs"]
mod formula;
#[path = "src/inline_vec.rs"]
mod inline_vec;
#[path = "src/notes.rs"]
mod notes;
#[path = "src/parse.rs"]
mod parse;
#[path = "src/symbol.rs"]
mod symbol;

use candidate_data_builder::{build_candidate_indices, build_candidate_records};
use candidate_record::{CandidateFormulaSummary, CandidateRecord};
use inline_vec::InlineVec;
use notes::{NoteLetter, NoteName, PitchSet};
use symbol::{
    Alteration, ChordSpec, Extension, MAX_ADDS, MAX_ALTERATIONS, MAX_OMISSIONS, Quality, Seventh,
};

fn main() -> io::Result<()> {
    for path in [
        "build.rs",
        "src/candidate_data_builder.rs",
        "src/candidate_record.rs",
        "src/formula.rs",
        "src/inline_vec.rs",
        "src/notes.rs",
        "src/parse.rs",
        "src/symbol.rs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    let records = build_candidate_records();
    let indices = build_candidate_indices(&records);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "OUT_DIR is not set for build script",
        )
    })?);
    let path = out_dir.join("identify_candidate_data.rs");
    let file = File::create(path)?;
    let mut out = BufWriter::new(file);

    writeln!(
        out,
        "use crate::candidate_record::{{CandidateFormulaSummary, CandidateRecord}};"
    )?;
    writeln!(out, "use crate::inline_vec::InlineVec;")?;
    writeln!(out, "use crate::notes::{{NoteLetter, NoteName, PitchSet}};")?;
    writeln!(
        out,
        "use crate::symbol::{{Alteration, ChordSpec, Extension, Quality, Seventh}};"
    )?;
    writeln!(
        out,
        "pub(super) static CANDIDATE_RECORDS: [CandidateRecord; {}] = [",
        records.len()
    )?;
    for record in &records {
        write_candidate_record(&mut out, record)?;
    }
    writeln!(out, "];")?;
    write_usize_array(
        &mut out,
        "EXACT_OFFSETS",
        &indices.exact_offsets,
        "pub(super) static",
    )?;
    write_u16_index_array(
        &mut out,
        "EXACT_INDICES",
        &indices.exact_indices,
        "pub(super) static",
    )?;
    write_usize_array(
        &mut out,
        "SUPERSET_OFFSETS",
        &indices.superset_offsets,
        "pub(super) static",
    )?;
    write_u16_index_array(
        &mut out,
        "SUPERSET_INDICES",
        &indices.superset_indices,
        "pub(super) static",
    )?;
    out.flush()
}

fn write_candidate_record(out: &mut impl Write, record: &CandidateRecord) -> io::Result<()> {
    writeln!(
        out,
        "CandidateRecord {{ root: {}, spec: {}, summary: {}, formula_set: PitchSet {{ bits: {} }} }},",
        note_code(record.root),
        spec_code(&record.spec),
        summary_code(record.summary),
        record.formula_set.bits,
    )
}

fn summary_code(summary: CandidateFormulaSummary) -> String {
    format!(
        "CandidateFormulaSummary {{ tone_count: {}, degree_by_pitch: {}, semitone_by_pitch: {} }}",
        summary.tone_count,
        u8_array_code(&summary.degree_by_pitch),
        i8_array_code(&summary.semitone_by_pitch),
    )
}

fn u8_array_code(values: &[u8; 12]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn i8_array_code(values: &[i8; 12]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(i8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn spec_code(spec: &ChordSpec) -> String {
    format!(
        "ChordSpec {{ quality: {}, seventh: {}, extension: {}, sixth: {}, alt: {}, adds: {}, alterations: {}, omissions: {} }}",
        quality_code(spec.quality),
        seventh_code(spec.seventh),
        extension_code(spec.extension),
        spec.sixth,
        spec.alt,
        inline_u8_code(spec.adds.as_slice(), MAX_ADDS),
        inline_alteration_code(spec.alterations.as_slice()),
        inline_u8_code(spec.omissions.as_slice(), MAX_OMISSIONS),
    )
}

fn inline_u8_code(values: &[u8], capacity: usize) -> String {
    let mut items = values.to_vec();
    items.resize(capacity, 0);
    format!(
        "InlineVec::from_parts({}, [{}])",
        values.len(),
        items
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn inline_alteration_code(values: &[Alteration]) -> String {
    let mut items = values.to_vec();
    items.resize(
        MAX_ALTERATIONS,
        Alteration {
            degree: 0,
            accidental: 0,
        },
    );
    format!(
        "InlineVec::from_parts({}, [{}])",
        values.len(),
        items
            .iter()
            .map(alteration_code)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn alteration_code(alteration: &Alteration) -> String {
    format!(
        "Alteration {{ degree: {}, accidental: {} }}",
        alteration.degree(),
        alteration.accidental()
    )
}

fn note_code(note: NoteName) -> String {
    format!(
        "NoteName::const_new({}, {})",
        letter_code(note.letter()),
        note.accidental()
    )
}

fn letter_code(letter: NoteLetter) -> &'static str {
    match letter {
        NoteLetter::C => "NoteLetter::C",
        NoteLetter::D => "NoteLetter::D",
        NoteLetter::E => "NoteLetter::E",
        NoteLetter::F => "NoteLetter::F",
        NoteLetter::G => "NoteLetter::G",
        NoteLetter::A => "NoteLetter::A",
        NoteLetter::B => "NoteLetter::B",
    }
}

fn quality_code(quality: Quality) -> &'static str {
    match quality {
        Quality::Major => "Quality::Major",
        Quality::Minor => "Quality::Minor",
        Quality::Diminished => "Quality::Diminished",
        Quality::Augmented => "Quality::Augmented",
        Quality::Sus2 => "Quality::Sus2",
        Quality::Sus4 => "Quality::Sus4",
        Quality::Power => "Quality::Power",
    }
}

fn seventh_code(seventh: Seventh) -> &'static str {
    match seventh {
        Seventh::None => "Seventh::None",
        Seventh::Minor => "Seventh::Minor",
        Seventh::Major => "Seventh::Major",
        Seventh::Diminished => "Seventh::Diminished",
    }
}

fn extension_code(extension: Option<Extension>) -> &'static str {
    match extension {
        Some(Extension::Ninth) => "Some(Extension::Ninth)",
        Some(Extension::Eleventh) => "Some(Extension::Eleventh)",
        Some(Extension::Thirteenth) => "Some(Extension::Thirteenth)",
        None => "None",
    }
}

fn write_usize_array(
    out: &mut impl Write,
    name: &str,
    values: &[usize],
    visibility: &str,
) -> io::Result<()> {
    writeln!(out, "{visibility} {name}: [usize; {}] = [", values.len())?;
    for chunk in values.chunks(16) {
        write!(out, "    ")?;
        for value in chunk {
            write!(out, "{value}, ")?;
        }
        writeln!(out)?;
    }
    writeln!(out, "];")
}

fn write_u16_index_array(
    out: &mut impl Write,
    name: &str,
    values: &[usize],
    visibility: &str,
) -> io::Result<()> {
    writeln!(out, "{visibility} {name}: [u16; {}] = [", values.len())?;
    for chunk in values.chunks(24) {
        write!(out, "    ")?;
        for value in chunk {
            let value = u16::try_from(*value).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("candidate index {value} exceeds u16 range"),
                )
            })?;
            write!(out, "{value}, ")?;
        }
        writeln!(out)?;
    }
    writeln!(out, "];")
}
