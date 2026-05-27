use std::process::ExitCode;
use std::{fmt, io};

use chordclaw_core::{
    AnalysisClass, ChordClawError, ChordClawErrorKind, DEFAULT_LIMIT, DEFAULT_MAX_FRET,
    DEFAULT_MAX_SPAN, DEFAULT_MIN_FRET, GuitarTuning, IdentifyResult, Instrument, MAX_LIMIT,
    MAX_STANDARD_FRET, VoicingMode, VoicingOptions, analyze_symbol, identify_with_tuning,
    voicings_with_tuning,
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
    let fingering = required_string(matches, "fingering")?;
    let instrument = optional_instrument(matches)?;
    let tuning = optional_tuning(matches, instrument)?;
    let result = identify_with_tuning(fingering, tuning)?;
    if matches.get_flag("json") {
        write_json(&result)
    } else {
        print_identify(&result)
    }
}

fn cmd_voicings(matches: &ArgMatches) -> Result<(), CliError> {
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
        write_json(&results)
    } else {
        print_voicings(&results)
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

fn write_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    if let Err(error) = serde_json::to_writer(&mut lock, value) {
        if error.io_error_kind() == Some(io::ErrorKind::BrokenPipe) {
            return Ok(());
        }
        return Err(CliError::internal(format!("write json: {error}")));
    }
    write_stdout(writeln!(lock))
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

fn print_identify(result: &IdentifyResult) -> Result<(), CliError> {
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

    Ok(())
}

fn print_voicings(results: &[chordclaw_core::Voicing]) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    if results.is_empty() {
        write_stdout(writeln!(out, "No voicings found."))?;
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
        .map(|result| result.notes.join(" ").len())
        .max()
        .unwrap_or(0)
        .max(24);
    if has_omissions {
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
        if has_omissions {
            write_stdout(writeln!(
                out,
                "{:<compact_width$} | {:<notes_width$} | {}",
                result.compact,
                result.notes.join(" "),
                result.omissions.join(",")
            ))?;
        } else {
            write_stdout(writeln!(
                out,
                "{:<compact_width$} | {}",
                result.compact,
                result.notes.join(" ")
            ))?;
        }
    }

    Ok(())
}
