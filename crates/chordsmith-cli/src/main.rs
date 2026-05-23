use std::io;
use std::process::ExitCode;

use chordsmith_core::{
    AnalysisClass, DEFAULT_LIMIT, DEFAULT_MAX_FRET, DEFAULT_MAX_SPAN, IdentifyResult,
    MAX_STANDARD_FRET, VoicingOptions, analyze_symbol, identify, voicings,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use clap_complete::{Shell, generate};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(exit_code_for_error(&error))
        }
    }
}

fn run() -> Result<(), String> {
    let matches = match cli().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            if code == 0 {
                return Ok(());
            }
            return Err(format!("clap exited with {code}"));
        }
    };

    match matches.subcommand() {
        Some(("identify", sub)) => cmd_identify(sub),
        Some(("voicings", sub)) => cmd_voicings(sub),
        Some(("analyze", sub)) => cmd_analyze(sub),
        Some(("completions", sub)) => cmd_completions(sub),
        Some((other, _)) => Err(format!("unknown subcommand after clap parse: {other}")),
        None => {
            let mut command = cli();
            command
                .print_help()
                .map_err(|error| format!("print help: {error}"))?;
            println!();
            Ok(())
        }
    }
}

fn exit_code_for_error(error: &str) -> u8 {
    if error.starts_with("usage:")
        || error.starts_with("unknown ")
        || error.starts_with("unsupported shell")
        || error.starts_with("clap exited with 2")
        || error.contains("cannot be used with")
    {
        return 2;
    }
    if error.starts_with("parse ")
        || error.starts_with("invalid ")
        || error.starts_with("expected ")
        || error.contains("chord descriptor")
    {
        return 65;
    }
    1
}

fn cli() -> Command {
    Command::new("chordsmith")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Guitar chord analysis and voicing CLI")
        .subcommand(
            Command::new("identify")
                .about("Identify a low-to-high guitar fingering")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON")
                        .action(ArgAction::SetTrue),
                )
                .arg(Arg::new("fingering").value_name("FINGERING").required(true)),
        )
        .subcommand(
            Command::new("voicings")
                .about("Generate guitar voicings from a chord symbol")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("max_fret")
                        .long("max-fret")
                        .value_name("FRET")
                        .help("Highest fret to scan, 0..=24"),
                )
                .arg(
                    Arg::new("max_span")
                        .long("max-span")
                        .value_name("FRETS")
                        .help("Maximum non-open fret span, 0..=24"),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("COUNT")
                        .help("Maximum voicings to print"),
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

fn cmd_identify(matches: &ArgMatches) -> Result<(), String> {
    let fingering = required_string(matches, "fingering")?;
    let result = identify(fingering).map_err(|error| error.to_string())?;
    if matches.get_flag("json") {
        write_json(&result)
    } else {
        print_identify(&result);
        Ok(())
    }
}

fn cmd_voicings(matches: &ArgMatches) -> Result<(), String> {
    let chord = required_string(matches, "chord")?;
    let all = matches.get_flag("all");
    if all && matches.get_one::<String>("limit").is_some() {
        return Err("--all cannot be used with --limit".to_owned());
    }
    let max_fret = optional_u8(matches, "max_fret", DEFAULT_MAX_FRET)?;
    if max_fret > MAX_STANDARD_FRET {
        return Err(format!(
            "invalid max_fret: '{max_fret}', standard guitar range is 0..={MAX_STANDARD_FRET}"
        ));
    }
    let max_span = optional_u8(matches, "max_span", DEFAULT_MAX_SPAN)?;
    if max_span > MAX_STANDARD_FRET {
        return Err(format!(
            "invalid max_span: '{max_span}', standard guitar range is 0..={MAX_STANDARD_FRET}"
        ));
    }
    let options = VoicingOptions {
        max_fret,
        max_span,
        limit: optional_usize(matches, "limit", DEFAULT_LIMIT)?,
        all,
    };

    let results = voicings(chord, options).map_err(|error| error.to_string())?;
    if matches.get_flag("json") {
        write_json(&results)
    } else {
        print_voicings(&results);
        Ok(())
    }
}

fn cmd_analyze(matches: &ArgMatches) -> Result<(), String> {
    let chord = required_string(matches, "chord")?;
    let (symbol, formula) = analyze_symbol(chord).map_err(|error| error.to_string())?;
    if matches.get_flag("json") {
        let output = serde_json::json!({
            "symbol": symbol.name(),
            "root": symbol.root.to_string(),
            "bass": symbol.bass.map(|bass| bass.to_string()),
            "tones": formula.tones,
        });
        write_json(&output)
    } else {
        let notes = formula
            .tones
            .iter()
            .map(|tone| tone.note.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let intervals = formula
            .tones
            .iter()
            .map(|tone| tone.interval.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        println!("{}", symbol.name());
        println!("Notes: {notes}");
        println!("Intervals: {intervals}");
        Ok(())
    }
}

fn cmd_completions(matches: &ArgMatches) -> Result<(), String> {
    let shell_text = required_string(matches, "shell")?;
    let shell = shell_text
        .parse::<Shell>()
        .map_err(|_| format!("unsupported shell '{shell_text}'"))?;
    let mut command = cli();
    let mut stdout = io::stdout();
    generate(shell, &mut command, "chordsmith", &mut stdout);
    Ok(())
}

fn required_string<'a>(matches: &'a ArgMatches, name: &str) -> Result<&'a str, String> {
    matches
        .get_one::<String>(name)
        .map(String::as_str)
        .ok_or_else(|| format!("missing required argument '{name}'"))
}

fn optional_u8(matches: &ArgMatches, name: &str, default: u8) -> Result<u8, String> {
    match matches.get_one::<String>(name) {
        Some(value) => value
            .parse::<u8>()
            .map_err(|_| format!("invalid {name}: '{value}'")),
        None => Ok(default),
    }
}

fn optional_usize(matches: &ArgMatches, name: &str, default: usize) -> Result<usize, String> {
    match matches.get_one::<String>(name) {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| format!("invalid {name}: '{value}'")),
        None => Ok(default),
    }
}

fn write_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)
        .map_err(|error| format!("write json: {error}"))?;
    println!();
    Ok(())
}

fn print_identify(result: &IdentifyResult) {
    println!("Input: {}", result.fingering);
    if result.notes.is_empty() {
        println!("Primary: Muted");
        return;
    }

    let notes = result
        .notes
        .iter()
        .map(|note| note.note.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    println!("Notes: {notes}");

    match &result.primary {
        Some(primary) => {
            println!("Primary: {}", primary.symbol);
            let useful_aliases = result
                .aliases
                .iter()
                .filter(|analysis| analysis.class == AnalysisClass::UsefulAlias)
                .take(8)
                .map(|analysis| analysis.symbol.as_str())
                .collect::<Vec<_>>();
            if !useful_aliases.is_empty() {
                let aliases = result
                    .aliases
                    .iter()
                    .filter(|analysis| analysis.class == AnalysisClass::UsefulAlias)
                    .take(8)
                    .map(|analysis| analysis.symbol.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ");
                println!("Also: {aliases}");
            }
        }
        None => println!("Primary: Unknown"),
    }
}

fn print_voicings(results: &[chordsmith_core::Voicing]) {
    if results.is_empty() {
        println!("No voicings found.");
        return;
    }

    let has_omissions = results.iter().any(|result| !result.omissions.is_empty());
    if has_omissions {
        println!("{:<12} | {:<24} | OMIT", "COMPACT", "NOTES");
        println!("{}", "-".repeat(48));
    } else {
        println!("{:<12} | NOTES", "COMPACT");
        println!("{}", "-".repeat(40));
    }
    for result in results {
        if has_omissions {
            println!(
                "{:<12} | {:<24} | {}",
                result.compact,
                result.notes.join(" "),
                result.omissions.join(",")
            );
        } else {
            println!("{:<12} | {}", result.compact, result.notes.join(" "));
        }
    }
}
