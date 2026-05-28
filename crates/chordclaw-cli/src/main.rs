use std::process::ExitCode;
use std::{fmt, io};

use chordclaw_core::{
    AnalysisClass, ChordAnalysis, ChordClawError, ChordClawErrorKind, DEFAULT_LIMIT,
    DEFAULT_MAX_FRET, DEFAULT_MAX_SPAN, DEFAULT_MIN_FRET, Fingering, GUITAR8_STRING_COUNT,
    GuitarTuning, IdentifyResult, Instrument, MAX_LIMIT, MAX_STANDARD_FRET, Voicing, VoicingMode,
    VoicingOptions, VoicingScoreBreakdown, VoicingScoreContext, analyze_symbol,
    identify_fingering_with_tuning, voicings_with_tuning,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_complete::{Shell, generate};
use io::Write;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn run() -> Result<ExitCode, CliError> {
    let matches = match cli().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)));
        }
    };

    match matches.subcommand() {
        Some(("identify", sub)) => cmd_identify(sub).map(|()| ExitCode::from(0)),
        Some(("voicings", sub)) => cmd_voicings(sub).map(|()| ExitCode::from(0)),
        Some(("analyze", sub)) => cmd_analyze(sub).map(|()| ExitCode::from(0)),
        Some(("completions", sub)) => cmd_completions(sub).map(|()| ExitCode::from(0)),
        Some((other, _)) => Err(CliError::internal(format!(
            "unknown subcommand after clap parse: {other}"
        ))),
        None => {
            let mut command = cli();
            command
                .print_help()
                .map_err(|error| CliError::internal(format!("print help: {error}")))?;
            let mut stdout = io::stdout().lock();
            write_stdout(writeln!(stdout))?;
            Ok(ExitCode::from(0))
        }
    }
}

#[derive(Debug)]
enum CliError {
    Core(ChordClawError),
    Message {
        kind: ChordClawErrorKind,
        message: String,
    },
}

impl CliError {
    fn data(message: impl Into<String>) -> Self {
        Self::message(ChordClawErrorKind::Data, message)
    }

    fn usage(message: impl Into<String>) -> Self {
        Self::message(ChordClawErrorKind::Usage, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::message(ChordClawErrorKind::Internal, message)
    }

    fn message(kind: ChordClawErrorKind, message: impl Into<String>) -> Self {
        Self::Message {
            kind,
            message: message.into(),
        }
    }

    fn kind(&self) -> ChordClawErrorKind {
        match self {
            Self::Core(error) => error.kind(),
            Self::Message { kind, .. } => *kind,
        }
    }

    fn exit_code(&self) -> u8 {
        match self.kind() {
            ChordClawErrorKind::Data => 65,
            ChordClawErrorKind::Usage => 2,
            ChordClawErrorKind::Internal => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(f),
            Self::Message { message, .. } => f.write_str(message),
        }
    }
}

impl From<ChordClawError> for CliError {
    fn from(value: ChordClawError) -> Self {
        Self::Core(value)
    }
}

fn cli() -> Command {
    Command::new("chordclaw")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Guitar and ukulele chord analysis and voicing CLI")
        .subcommand(
            Command::new("identify")
                .about("Identify a string-order fretted instrument fingering")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("explain")
                        .long("explain")
                        .help("Print ranked analysis candidates with scores")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("diagram")
                        .long("diagram")
                        .help("Print an ASCII string diagram")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("instrument")
                        .long("instrument")
                        .value_name("INSTRUMENT")
                        .help("Instrument to use: guitar, guitar7, guitar8, or ukulele"),
                )
                .arg(Arg::new("tuning").long("tuning").value_name("TUNING").help(
                    "String-order tuning, e.g. EADGBE, BEADGBE, F#BEADGBE, GCEA, or G3,C4,E4,A4",
                ))
                .arg(Arg::new("fingering").value_name("FINGERING").required(true)),
        )
        .subcommand(
            Command::new("voicings")
                .about("Generate fretted instrument voicings from a chord symbol")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("explain")
                        .long("explain")
                        .help("Print voicing score breakdowns")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("diagram")
                        .long("diagram")
                        .help("Print ASCII string diagrams")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("instrument")
                        .long("instrument")
                        .value_name("INSTRUMENT")
                        .help("Instrument to use: guitar, guitar7, guitar8, or ukulele"),
                )
                .arg(Arg::new("tuning").long("tuning").value_name("TUNING").help(
                    "String-order tuning, e.g. EADGBE, BEADGBE, F#BEADGBE, GCEA, or G3,C4,E4,A4",
                ))
                .arg(
                    Arg::new("min_fret")
                        .long("min-fret")
                        .value_name("FRET")
                        .help("Lowest fret to scan, 0..=30; values above 0 exclude open strings"),
                )
                .arg(
                    Arg::new("max_fret")
                        .long("max-fret")
                        .value_name("FRET")
                        .help("Highest fret to scan, 0..=30"),
                )
                .arg(
                    Arg::new("max_span")
                        .long("max-span")
                        .value_name("FRETS")
                        .help("Maximum non-open fret span, 0..=30"),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Maximum curated voicings to print, 0..=1000"),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .help("Print every generated voicing instead of the curated limit")
                        .action(ArgAction::SetTrue),
                )
                .arg(Arg::new("chord").value_name("CHORD").required(true)),
        )
        .subcommand(
            Command::new("analyze")
                .about("Parse a chord symbol and print its notes and intervals")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON")
                        .action(ArgAction::SetTrue),
                )
                .arg(Arg::new("chord").value_name("CHORD").required(true)),
        )
        .subcommand(
            Command::new("completions")
                .about("Generate shell completion script")
                .arg(Arg::new("shell").value_name("SHELL").required(true)),
        )
}

fn cmd_identify(matches: &ArgMatches) -> Result<(), CliError> {
    if matches.get_flag("json") && matches.get_flag("diagram") {
        return Err(CliError::usage("--diagram cannot be used with --json"));
    }
    let fingering_text = required_string(matches, "fingering")?;
    let instrument = optional_instrument(matches)?;
    let tuning = identify_tuning(matches, instrument, fingering_text)?;
    let fingering = Fingering::parse_with_string_count(fingering_text, tuning.string_count())?;
    let result = identify_fingering_with_tuning(&fingering, tuning)?;
    if matches.get_flag("json") {
        write_json(&result)
    } else {
        print_identify(
            &result,
            &fingering,
            tuning,
            matches.get_flag("explain"),
            matches.get_flag("diagram"),
        )
    }
}

fn cmd_voicings(matches: &ArgMatches) -> Result<(), CliError> {
    if matches.get_flag("json") && matches.get_flag("diagram") {
        return Err(CliError::usage("--diagram cannot be used with --json"));
    }
    let chord = required_string(matches, "chord")?;
    let instrument = optional_instrument(matches)?;
    let tuning = optional_tuning(matches, instrument)?;
    let all = matches.get_flag("all");
    if all && matches.get_one::<String>("limit").is_some() {
        return Err(CliError::usage("--all cannot be used with --limit"));
    }
    let min_fret = optional_u8(matches, "min_fret", DEFAULT_MIN_FRET)?;
    let default_max_fret =
        if matches.get_one::<String>("max_fret").is_none() && min_fret >= DEFAULT_MAX_FRET {
            MAX_STANDARD_FRET
        } else {
            DEFAULT_MAX_FRET
        };
    let max_fret = optional_u8(matches, "max_fret", default_max_fret)?;
    let max_span = optional_u8(matches, "max_span", DEFAULT_MAX_SPAN)?;
    let limit = optional_usize(matches, "limit", DEFAULT_LIMIT)?;
    if limit > MAX_LIMIT {
        return Err(CliError::usage(format!(
            "invalid limit '{limit}': curated voicing limit is 0..={MAX_LIMIT}; use --all for exhaustive output"
        )));
    }
    let options = VoicingOptions {
        min_fret,
        max_fret,
        max_span,
        mode: if all {
            VoicingMode::All
        } else {
            VoicingMode::Curated { limit }
        },
    };

    let results = voicings_with_tuning(chord, tuning, options)?;
    if matches.get_flag("json") {
        if matches.get_flag("explain") {
            write_explained_voicings_json(chord, tuning, &results)
        } else {
            write_json(&results)
        }
    } else {
        print_voicings(
            chord,
            tuning,
            &results,
            matches.get_flag("explain"),
            matches.get_flag("diagram"),
        )
    }
}

fn cmd_analyze(matches: &ArgMatches) -> Result<(), CliError> {
    let chord = required_string(matches, "chord")?;
    let (symbol, formula) = analyze_symbol(chord)?;
    let notes = analysis_notes(&symbol, &formula);
    let intervals = analysis_intervals(&symbol, &formula);
    if matches.get_flag("json") {
        let output = serde_json::json!({
            "symbol": symbol.name(),
            "root": symbol.root().to_string(),
            "bass": symbol.bass().map(|bass| bass.to_string()),
            "notes": notes,
            "intervals": intervals,
            "tones": formula.tones(),
        });
        write_json(&output)
    } else {
        print_analyze(&symbol, &notes, &intervals)
    }
}

fn analysis_notes(
    symbol: &chordclaw_core::ChordSymbol,
    formula: &chordclaw_core::ChordFormula,
) -> Vec<String> {
    let mut notes = formula
        .tones()
        .iter()
        .map(|tone| tone.note.to_string())
        .collect::<Vec<_>>();
    if let Some(bass) = symbol.bass()
        && !formula
            .tones()
            .iter()
            .any(|tone| tone.pitch_class == bass.pitch_class().value())
    {
        notes.push(bass.to_string());
    }
    notes
}

fn analysis_intervals(
    symbol: &chordclaw_core::ChordSymbol,
    formula: &chordclaw_core::ChordFormula,
) -> Vec<String> {
    let mut intervals = formula
        .tones()
        .iter()
        .map(|tone| tone.interval.to_string())
        .collect::<Vec<_>>();
    if let Some(bass) = symbol.bass()
        && !formula
            .tones()
            .iter()
            .any(|tone| tone.pitch_class == bass.pitch_class().value())
    {
        intervals.push("bass".to_owned());
    }
    intervals
}

fn cmd_completions(matches: &ArgMatches) -> Result<(), CliError> {
    let shell_text = required_string(matches, "shell")?;
    let shell = shell_text
        .parse::<Shell>()
        .map_err(|_| CliError::usage(format!("unsupported shell '{shell_text}'")))?;
    let mut command = cli();
    let mut stdout = io::stdout();
    generate(shell, &mut command, "chordclaw", &mut stdout);
    Ok(())
}

fn required_string<'a>(matches: &'a ArgMatches, name: &str) -> Result<&'a str, CliError> {
    matches
        .get_one::<String>(name)
        .map(String::as_str)
        .ok_or_else(|| CliError::usage(format!("missing required argument '{name}'")))
}

fn optional_u8(matches: &ArgMatches, name: &str, default: u8) -> Result<u8, CliError> {
    match matches.get_one::<String>(name) {
        Some(value) => value
            .parse::<u8>()
            .map_err(|_| CliError::data(format!("invalid {name}: '{value}'"))),
        None => Ok(default),
    }
}

fn optional_usize(matches: &ArgMatches, name: &str, default: usize) -> Result<usize, CliError> {
    match matches.get_one::<String>(name) {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| CliError::data(format!("invalid {name}: '{value}'"))),
        None => Ok(default),
    }
}

fn optional_instrument(matches: &ArgMatches) -> Result<Option<Instrument>, CliError> {
    match matches.get_one::<String>("instrument") {
        Some(value) => Instrument::parse(value).map(Some).map_err(CliError::from),
        None => Ok(None),
    }
}

fn optional_tuning(
    matches: &ArgMatches,
    instrument: Option<Instrument>,
) -> Result<GuitarTuning, CliError> {
    match matches.get_one::<String>("tuning") {
        Some(value) => match instrument {
            Some(instrument) => {
                GuitarTuning::parse_for_instrument(value, instrument).map_err(CliError::from)
            }
            None => GuitarTuning::parse(value).map_err(CliError::from),
        },
        None => Ok(instrument.unwrap_or(Instrument::Guitar).default_tuning()),
    }
}

fn identify_tuning(
    matches: &ArgMatches,
    instrument: Option<Instrument>,
    fingering_text: &str,
) -> Result<GuitarTuning, CliError> {
    if matches.get_one::<String>("tuning").is_some() {
        return optional_tuning(matches, instrument);
    }

    let instrument = match instrument {
        Some(instrument) => instrument,
        None => {
            let string_count = Fingering::string_count_from_input(fingering_text)?;
            Instrument::from_string_count(string_count)?
        }
    };
    Ok(instrument.default_tuning())
}

fn write_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    write_json_to(&mut lock, value)?;
    write_stdout(writeln!(lock))
}

fn write_json_to<T: serde::Serialize>(out: &mut impl Write, value: &T) -> Result<(), CliError> {
    if let Err(error) = serde_json::to_writer(out, value) {
        if error.io_error_kind() == Some(io::ErrorKind::BrokenPipe) {
            return Ok(());
        }
        return Err(CliError::internal(format!("write json: {error}")));
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct ExplainedVoicing<'a> {
    #[serde(flatten)]
    voicing: &'a Voicing,
    score_breakdown: VoicingScoreBreakdown,
}

fn write_explained_voicings_json(
    chord: &str,
    tuning: GuitarTuning,
    results: &[Voicing],
) -> Result<(), CliError> {
    let score_context = VoicingScoreContext::new(chord, tuning)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_stdout(write!(out, "["))?;
    for (idx, result) in results.iter().enumerate() {
        if idx > 0 {
            write_stdout(write!(out, ","))?;
        }
        let score_breakdown = score_context.breakdown(&result.frets)?;
        write_json_to(
            &mut out,
            &ExplainedVoicing {
                voicing: result,
                score_breakdown,
            },
        )?;
    }
    write_stdout(writeln!(out, "]"))
}

fn write_stdout(result: io::Result<()>) -> Result<(), CliError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(CliError::internal(format!("write stdout: {error}"))),
    }
}

fn print_analyze(
    symbol: &chordclaw_core::ChordSymbol,
    notes: &[String],
    intervals: &[String],
) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_stdout(writeln!(out, "{}", symbol.name()))?;
    write_stdout(writeln!(out, "Notes: {}", notes.join(" ")))?;
    write_stdout(writeln!(out, "Intervals: {}", intervals.join(" ")))
}

fn print_identify(
    result: &IdentifyResult,
    fingering: &Fingering,
    tuning: GuitarTuning,
    explain: bool,
    diagram: bool,
) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_stdout(writeln!(out, "Input: {}", result.fingering))?;
    if result.notes.is_empty() {
        write_stdout(writeln!(out, "Primary: Muted"))?;
        return Ok(());
    }

    let notes = result
        .notes
        .iter()
        .map(|note| note.note.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    write_stdout(writeln!(out, "Notes: {notes}"))?;

    match &result.primary {
        Some(primary) => {
            write_stdout(writeln!(out, "Primary: {}", primary.symbol))?;
            if !primary.omissions.is_empty() {
                write_stdout(writeln!(out, "Omit: {}", primary.omissions.join(",")))?;
            }
            let aliases = result
                .aliases
                .iter()
                .filter(|analysis| analysis.class == AnalysisClass::UsefulAlias)
                .take(8)
                .map(|analysis| analysis.symbol.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            if !aliases.is_empty() {
                write_stdout(writeln!(out, "Also: {aliases}"))?;
            }
        }
        None => write_stdout(writeln!(out, "Primary: Unknown"))?,
    }

    if explain {
        print_identify_explain(&mut out, result)?;
    }
    if diagram {
        write_stdout(writeln!(out))?;
        print_identify_diagram(&mut out, fingering, tuning, result)?;
    }

    Ok(())
}

fn print_identify_explain(out: &mut impl Write, result: &IdentifyResult) -> Result<(), CliError> {
    write_stdout(writeln!(out, "Candidates:"))?;
    if let Some(primary) = &result.primary {
        print_analysis_candidate(out, 1, primary)?;
    }
    for (idx, analysis) in result.aliases.iter().take(8).enumerate() {
        print_analysis_candidate(out, idx + 2, analysis)?;
    }
    Ok(())
}

fn print_analysis_candidate(
    out: &mut impl Write,
    idx: usize,
    analysis: &ChordAnalysis,
) -> Result<(), CliError> {
    write_stdout(write!(
        out,
        "  {idx}. {} score={} class={:?} confidence={:?} intervals=",
        analysis.symbol, analysis.score, analysis.class, analysis.confidence
    ))?;
    write_stdout(write_joined_strings(out, &analysis.intervals, " "))?;
    if !analysis.omissions.is_empty() {
        write_stdout(write!(out, " omit="))?;
        write_stdout(write_joined_strings(out, &analysis.omissions, ","))?;
    }
    write_stdout(writeln!(out))
}

fn print_voicings(
    chord: &str,
    tuning: GuitarTuning,
    results: &[Voicing],
    explain: bool,
    diagram: bool,
) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    if results.is_empty() {
        write_stdout(writeln!(out, "No voicings found."))?;
        return Ok(());
    }

    let score_context = if explain {
        Some(VoicingScoreContext::new(chord, tuning)?)
    } else {
        None
    };

    if diagram {
        for (idx, result) in results.iter().enumerate() {
            if idx > 0 {
                write_stdout(writeln!(out))?;
            }
            print_voicing_diagram_block(&mut out, tuning, result, score_context.as_ref())?;
        }
        return Ok(());
    }

    let has_omissions = results.iter().any(|result| !result.omissions.is_empty());
    let compact_width = results
        .iter()
        .map(|result| result.compact.len())
        .max()
        .unwrap_or(0)
        .max(12);
    let notes_width = results
        .iter()
        .map(|result| joined_string_len(&result.notes, 1))
        .max()
        .unwrap_or(0)
        .max(24);
    let score_width = if explain {
        results
            .iter()
            .map(|result| decimal_width(result.score))
            .max()
            .unwrap_or(0)
            .max(5)
    } else {
        0
    };
    if explain && has_omissions {
        write_stdout(writeln!(
            out,
            "{:<compact_width$} | {:>score_width$} | {:<notes_width$} | OMIT",
            "COMPACT", "SCORE", "NOTES"
        ))?;
        write_stdout(writeln!(
            out,
            "{}",
            "-".repeat(compact_width + score_width + notes_width + 13)
        ))?;
    } else if explain {
        write_stdout(writeln!(
            out,
            "{:<compact_width$} | {:>score_width$} | NOTES",
            "COMPACT", "SCORE"
        ))?;
        write_stdout(writeln!(
            out,
            "{}",
            "-".repeat(compact_width + score_width + 11)
        ))?;
    } else if has_omissions {
        write_stdout(writeln!(
            out,
            "{:<compact_width$} | {:<notes_width$} | OMIT",
            "COMPACT", "NOTES"
        ))?;
        write_stdout(writeln!(
            out,
            "{}",
            "-".repeat(compact_width + notes_width + 10)
        ))?;
    } else {
        write_stdout(writeln!(out, "{:<compact_width$} | NOTES", "COMPACT"))?;
        write_stdout(writeln!(out, "{}", "-".repeat(compact_width + 8)))?;
    }
    for result in results {
        if explain && has_omissions {
            write_stdout(write!(
                out,
                "{:<compact_width$} | {:>score_width$} | ",
                result.compact, result.score
            ))?;
            write_padded_joined_strings(&mut out, &result.notes, " ", notes_width)?;
            write_stdout(write!(out, " | "))?;
            write_stdout(write_joined_strings(&mut out, &result.omissions, ","))?;
            write_stdout(writeln!(out))?;
        } else if explain {
            write_stdout(write!(
                out,
                "{:<compact_width$} | {:>score_width$} | ",
                result.compact, result.score
            ))?;
            write_stdout(write_joined_strings(&mut out, &result.notes, " "))?;
            write_stdout(writeln!(out))?;
        } else if has_omissions {
            write_stdout(write!(out, "{:<compact_width$} | ", result.compact))?;
            write_padded_joined_strings(&mut out, &result.notes, " ", notes_width)?;
            write_stdout(write!(out, " | "))?;
            write_stdout(write_joined_strings(&mut out, &result.omissions, ","))?;
            write_stdout(writeln!(out))?;
        } else {
            write_stdout(write!(out, "{:<compact_width$} | ", result.compact))?;
            write_stdout(write_joined_strings(&mut out, &result.notes, " "))?;
            write_stdout(writeln!(out))?;
        }
    }

    if explain {
        write_stdout(writeln!(out))?;
        write_stdout(writeln!(out, "Score breakdown:"))?;
        if let Some(score_context) = &score_context {
            for result in results {
                let breakdown = score_context.breakdown(&result.frets)?;
                write_score_breakdown(&mut out, result.compact.as_str(), &breakdown)?;
            }
        }
    }

    Ok(())
}

fn print_voicing_diagram_block(
    out: &mut impl Write,
    tuning: GuitarTuning,
    result: &Voicing,
    score_context: Option<&VoicingScoreContext>,
) -> Result<(), CliError> {
    write_stdout(write!(out, "{}", result.compact))?;
    if score_context.is_some() {
        write_stdout(write!(out, " score={}", result.score))?;
    }
    if !result.omissions.is_empty() {
        write_stdout(write!(out, " omit="))?;
        write_stdout(write_joined_strings(out, &result.omissions, ","))?;
    }
    write_stdout(write!(out, " notes="))?;
    write_stdout(write_joined_strings(out, &result.notes, " "))?;
    write_stdout(writeln!(out))?;
    print_voicing_diagram(out, tuning, result)?;
    if let Some(score_context) = score_context {
        let breakdown = score_context.breakdown(&result.frets)?;
        write_score_breakdown(out, result.compact.as_str(), &breakdown)?;
    }
    Ok(())
}

fn print_identify_diagram(
    out: &mut impl Write,
    fingering: &Fingering,
    tuning: GuitarTuning,
    result: &IdentifyResult,
) -> Result<(), CliError> {
    write_stdout(writeln!(out, "Diagram:"))?;
    for string in (0..tuning.string_count()).rev() {
        write_stdout(write!(out, "{:>3}|", tuning.notes()[string]))?;
        write_fret_cell(out, fingering.frets()[string])?;
        if let Some(note) = result.notes.iter().find(|note| note.string == string) {
            write_stdout(write!(out, " {}", note.note))?;
        }
        write_stdout(writeln!(out))?;
    }
    Ok(())
}

fn print_voicing_diagram(
    out: &mut impl Write,
    tuning: GuitarTuning,
    result: &Voicing,
) -> Result<(), CliError> {
    let mut note_by_string = [None::<&str>; GUITAR8_STRING_COUNT];
    let mut note_idx = 0usize;
    for (string, fret) in result.frets.iter().enumerate() {
        if fret.is_some() {
            note_by_string[string] = result.notes.get(note_idx).map(String::as_str);
            note_idx += 1;
        }
    }

    for string in (0..tuning.string_count()).rev() {
        write_stdout(write!(out, "{:>3}|", tuning.notes()[string]))?;
        write_fret_cell(out, result.frets[string])?;
        if let Some(note) = note_by_string[string] {
            write_stdout(write!(out, " {note}"))?;
        }
        write_stdout(writeln!(out))?;
    }
    Ok(())
}

fn write_fret_cell(out: &mut impl Write, fret: Option<u8>) -> Result<(), CliError> {
    match fret {
        Some(fret) => write_stdout(write!(out, "--{fret:>2}--")),
        None => write_stdout(write!(out, "-- x--")),
    }
}

fn write_score_breakdown(
    out: &mut impl Write,
    compact: &str,
    breakdown: &VoicingScoreBreakdown,
) -> Result<(), CliError> {
    write_stdout(write!(
        out,
        "{compact}: score total={} costs(",
        breakdown.total
    ))?;
    let mut wrote = false;
    write_component(out, &mut wrote, "position", breakdown.position_cost)?;
    write_component(out, &mut wrote, "relative", breakdown.relative_fret_cost)?;
    write_component(out, &mut wrote, "span", breakdown.fret_span_cost)?;
    write_component(out, &mut wrote, "strings", breakdown.active_string_cost)?;
    write_component(
        out,
        &mut wrote,
        "internal_mute",
        breakdown.internal_mute_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "adjacent_jump",
        breakdown.adjacent_fret_jump_cost,
    )?;
    write_component(out, &mut wrote, "duplicate", breakdown.duplicate_pitch_cost)?;
    write_component(out, &mut wrote, "high_open", breakdown.high_open_mix_cost)?;
    write_component(out, &mut wrote, "low_open_gap", breakdown.low_open_gap_cost)?;
    write_component(
        out,
        &mut wrote,
        "bass_mismatch",
        breakdown.preferred_bass_mismatch_cost,
    )?;
    write_component(out, &mut wrote, "omission", breakdown.harmonic_defect_cost)?;
    write_component(
        out,
        &mut wrote,
        "mute_quality",
        breakdown.internal_mute_quality_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "trailing_mute",
        breakdown.trailing_mute_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "sparse_duplicate",
        breakdown.sparse_duplicate_pitch_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "open_bypass",
        breakdown.open_chord_tone_bypass_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "low_ninth",
        breakdown.low_added_ninth_cluster_cost,
    )?;
    write_component(
        out,
        &mut wrote,
        "fingers",
        breakdown.fingering_complexity_cost,
    )?;
    write_component(out, &mut wrote, "instrument", breakdown.instrument_cost)?;
    if !wrote {
        write_stdout(write!(out, "none"))?;
    }
    write_stdout(write!(out, ") bonuses("))?;
    wrote = false;
    write_component(
        out,
        &mut wrote,
        "open_position",
        breakdown.open_position_bonus,
    )?;
    write_component(out, &mut wrote, "open_root", breakdown.open_root_bass_bonus)?;
    write_component(out, &mut wrote, "open_bass", breakdown.open_bass_grip_bonus)?;
    write_component(out, &mut wrote, "closed", breakdown.closed_shape_bonus)?;
    write_component(out, &mut wrote, "barre", breakdown.barre_grip_bonus)?;
    write_component(
        out,
        &mut wrote,
        "compact_low",
        breakdown.compact_low_grip_bonus,
    )?;
    write_component(out, &mut wrote, "jazz_shell", breakdown.jazz_shell_bonus)?;
    write_component(out, &mut wrote, "instrument", breakdown.instrument_bonus)?;
    if !wrote {
        write_stdout(write!(out, "none"))?;
    }
    write_stdout(writeln!(out, ")"))
}

fn write_component(
    out: &mut impl Write,
    wrote: &mut bool,
    name: &str,
    value: u32,
) -> Result<(), CliError> {
    if value == 0 {
        return Ok(());
    }
    if *wrote {
        write_stdout(write!(out, ", "))?;
    }
    *wrote = true;
    write_stdout(write!(out, "{name}={value}"))
}

fn joined_string_len(items: &[String], separator_len: usize) -> usize {
    if items.is_empty() {
        return 0;
    }
    items.iter().map(String::len).sum::<usize>() + separator_len * (items.len() - 1)
}

fn write_padded_joined_strings(
    out: &mut impl Write,
    items: &[String],
    separator: &str,
    width: usize,
) -> Result<(), CliError> {
    let len = joined_string_len(items, separator.len());
    write_stdout(write_joined_strings(out, items, separator))?;
    write_padding(out, width.saturating_sub(len))
}

fn write_joined_strings(out: &mut impl Write, items: &[String], separator: &str) -> io::Result<()> {
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            out.write_all(separator.as_bytes())?;
        }
        out.write_all(item.as_bytes())?;
    }
    Ok(())
}

fn write_padding(out: &mut impl Write, mut count: usize) -> Result<(), CliError> {
    const SPACES: &[u8; 32] = b"                                ";
    while count >= SPACES.len() {
        write_stdout(out.write_all(SPACES))?;
        count -= SPACES.len();
    }
    write_stdout(out.write_all(&SPACES[..count]))
}

const fn decimal_width(mut value: u32) -> usize {
    let mut width = 1usize;
    while value >= 10 {
        value /= 10;
        width += 1;
    }
    width
}
