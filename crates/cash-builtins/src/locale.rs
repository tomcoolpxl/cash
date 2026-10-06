//! `locale`, glibc's: what the locale is. Bare, it prints `LANG`, `LANGUAGE`, each
//! `LC_*` category and `LC_ALL` as glibc does: a category set in the environment is
//! printed bare, `LC_TIME=C`; one that follows `LC_ALL` or `LANG` is quoted,
//! `LC_TIME="C.UTF-8"`; and cash's default, with nothing set, is `C.UTF-8`, the one
//! locale it has. `-a` lists `C`, `C.UTF-8`, `POSIX` and the Windows locales as
//! `ll_CC.UTF-8` names: the user's regional format, the system's locale, the two display
//! languages, and `en_US.UTF-8`, which every script expects. `-m` lists `UTF-8`.
//!
//! The Cygwin `locale` of Git for Windows adds options that ask Windows: `-u`, `-s` and
//! `-f` print the user's display language, the system's and the user's regional format
//! as `ll_CC`, `-U` with `.UTF-8` attached; those are here too. There are no locale
//! definitions to look inside, so `-k`, `-c` and a NAME are refused politely, status 1.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Show the locale.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct LocaleCommand {
    /// Options and names, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The one locale cash has.
const DEFAULT: &str = "C.UTF-8";

/// glibc's categories, in the order `locale` prints them.
const CATEGORIES: [&str; 12] = [
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_MESSAGES",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
];

const HELP: &str = "\
Usage: locale [-amvhV]
   or: locale [-ck] NAME
   or: locale [-usfU]

Get locale-specific information.

System information:

  -a, --all-locales    List all available supported locales
  -m, --charmaps       List all available character maps
  -v, --verbose        More verbose output

Modify output format:

  -c, --category-name  List information about given category NAME
  -k, --keyword-name   Print information about given keyword NAME

Default locale information:

  -u, --user           Print locale of user's default UI language
  -s, --system         Print locale of system default UI language
  -f, --format         Print locale of user's regional format settings
                       (time, numeric & monetary)
  -U, --utf            Attach \".UTF-8\" to the result

Other options:

  -h, --help           This text
  -V, --version        Print program version and exit

cash has one locale, C.UTF-8, and no locale definitions to look inside:
NAME, -c and -k are refused. The Windows locales are listed by -a as
ll_CC.UTF-8 names.";

/// What is asked for.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    Help,
    Version,
    /// The environment's view, as bare `locale` prints it.
    Environment,
    /// `-a`: the locales.
    Locales,
    /// `-m`: the character maps.
    Charmaps,
    /// `-u`, `-s`, `-f`: one of Windows' locales, `-U` with `.UTF-8`.
    Windows(Which, bool),
    /// `-k`, `-c` or a name: definitions cash has none of.
    Definitions(String),
    /// An option that is not one.
    Unknown(String),
}

/// Which Windows locale `-u`, `-s` and `-f` ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    UserUi,
    SystemUi,
    Format,
}

/// The command line as glibc's `getopt_long` reads it, options anywhere.
fn parse(args: &[String]) -> Request {
    let mut request = Request::Environment;
    let mut utf = false;
    let mut names: Vec<&str> = Vec::new();
    let mut only_names = false;
    let set = |request: &mut Request, new: Request| {
        if matches!(request, Request::Environment) {
            *request = new;
        }
    };
    for arg in args {
        let arg = arg.as_str();
        if only_names || !arg.starts_with('-') || arg == "-" {
            names.push(arg);
            continue;
        }
        let flags: Vec<String> = if let Some(long) = arg.strip_prefix("--") {
            vec![long.to_owned()]
        } else if arg == "--" {
            only_names = true;
            continue;
        } else {
            arg.chars().skip(1).map(|c| c.to_string()).collect()
        };
        for flag in flags {
            match flag.as_str() {
                "h" | "help" => return Request::Help,
                "V" | "version" => return Request::Version,
                "a" | "all-locales" => set(&mut request, Request::Locales),
                "m" | "charmaps" => set(&mut request, Request::Charmaps),
                "v" | "verbose" => {}
                "u" | "user" => set(&mut request, Request::Windows(Which::UserUi, false)),
                "s" | "system" => set(&mut request, Request::Windows(Which::SystemUi, false)),
                "f" | "format" => set(&mut request, Request::Windows(Which::Format, false)),
                "U" | "utf" => utf = true,
                "c" | "category-name" | "k" | "keyword-name" => {
                    set(&mut request, Request::Definitions(format!("-{flag}")));
                }
                "i" | "input" | "n" | "no-unicode" => {
                    set(&mut request, Request::Definitions(format!("-{flag}")));
                }
                _ => return Request::Unknown(arg.to_owned()),
            }
        }
    }
    if let Request::Windows(which, _) = request {
        return Request::Windows(which, utf);
    }
    if let Some(name) = names.first()
        && matches!(request, Request::Environment)
    {
        return Request::Definitions((*name).to_owned());
    }
    request
}

/// The environment's view: each line `locale` prints, given a lookup of the variables.
fn environment_lines(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let lang = get("LANG").unwrap_or_default();
    let lc_all = get("LC_ALL").unwrap_or_default();
    let mut lines = vec![
        format!("LANG={lang}"),
        format!("LANGUAGE={}", get("LANGUAGE").unwrap_or_default()),
    ];
    for category in CATEGORIES {
        let line = if !lc_all.is_empty() {
            format!("{category}=\"{lc_all}\"")
        } else if let Some(value) = get(category).filter(|value| !value.is_empty()) {
            format!("{category}={value}")
        } else if !lang.is_empty() {
            format!("{category}=\"{lang}\"")
        } else {
            format!("{category}=\"{DEFAULT}\"")
        };
        lines.push(line);
    }
    lines.push(format!("LC_ALL={lc_all}"));
    lines
}

/// A Windows locale name as a POSIX one: `en-US` is `en_US`, `sr-Latn-RS` is `sr_RS`
/// (the script is dropped), `en` alone stays `en`.
fn posix_name(windows: &str) -> String {
    let mut parts = windows.split('-');
    let language = parts.next().unwrap_or_default();
    let region = parts.next_back().filter(|part| part.len() <= 3);
    match region {
        Some(region) if region != language => format!("{language}_{region}"),
        _ => language.to_owned(),
    }
}

/// `-a`: `C`, `C.UTF-8`, `POSIX`, then the Windows locales and `en_US.UTF-8`, as
/// `ll_CC.UTF-8` names in order, each once.
fn locales(windows: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = windows
        .into_iter()
        .map(|name| format!("{}.UTF-8", posix_name(&name)))
        .chain(std::iter::once("en_US.UTF-8".to_owned()))
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut all = vec!["C".to_owned(), "C.UTF-8".to_owned(), "POSIX".to_owned()];
    all.extend(names);
    all
}

impl builtins::Command for LocaleCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();
        match parse(&self.args) {
            Request::Help => {
                writeln!(stdout, "{HELP}")?;
            }
            Request::Version => {
                writeln!(
                    stdout,
                    "locale (cash): glibc's interface; one locale, C.UTF-8, and Windows' names"
                )?;
            }
            Request::Environment => {
                let get = |name: &str| context.shell.env_str(name).map(|v| v.into_owned());
                for line in environment_lines(get) {
                    writeln!(stdout, "{line}")?;
                }
            }
            Request::Locales => {
                let windows = [
                    cash_win32::locale::user_locale_name(),
                    cash_win32::locale::system_locale_name(),
                    cash_win32::locale::user_ui_locale_name(),
                    cash_win32::locale::system_ui_locale_name(),
                ];
                for name in locales(windows.into_iter().flatten()) {
                    writeln!(stdout, "{name}")?;
                }
            }
            Request::Charmaps => {
                writeln!(stdout, "UTF-8")?;
            }
            Request::Windows(which, utf) => {
                let name = match which {
                    Which::UserUi => cash_win32::locale::user_ui_locale_name(),
                    Which::SystemUi => cash_win32::locale::system_ui_locale_name(),
                    Which::Format => cash_win32::locale::user_locale_name(),
                };
                let Some(name) = name else {
                    writeln!(context.stderr(), "locale: Windows did not name its locale")?;
                    return Ok(ExecutionResult::general_error());
                };
                let suffix = if utf { ".UTF-8" } else { "" };
                writeln!(stdout, "{}{suffix}", posix_name(&name))?;
            }
            Request::Definitions(name) => {
                writeln!(
                    context.stderr(),
                    "locale: {name}: cash has no locale definitions to show; \
                     it has one locale, C.UTF-8 (see 'locale -a')"
                )?;
                return Ok(ExecutionResult::general_error());
            }
            Request::Unknown(option) => {
                let shown = option.strip_prefix("--").map_or_else(
                    || {
                        format!(
                            "invalid option -- '{}'",
                            option.chars().nth(1).unwrap_or('-')
                        )
                    },
                    |long| format!("unrecognized option '--{long}'"),
                );
                writeln!(
                    context.stderr(),
                    "locale: {shown}\nTry 'locale --help' for more information."
                )?;
                return Ok(ExecutionResult::general_error());
            }
        }
        Ok(ExecutionResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_environment_view_quotes_what_is_derived() {
        let env = |vars: &[(&str, &str)]| {
            let vars: Vec<(String, String)> = vars
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            environment_lines(move |name| {
                vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
            })
        };
        let lines = env(&[]);
        assert_eq!(lines[0], "LANG=");
        assert_eq!(lines[1], "LANGUAGE=");
        assert_eq!(lines[2], "LC_CTYPE=\"C.UTF-8\"");
        assert_eq!(lines.last().map(String::as_str), Some("LC_ALL="));
        assert_eq!(lines.len(), 15);

        let lines = env(&[("LANG", "en_US.UTF-8"), ("LC_TIME", "C")]);
        assert_eq!(lines[0], "LANG=en_US.UTF-8");
        assert_eq!(lines[2], "LC_CTYPE=\"en_US.UTF-8\"");
        assert_eq!(lines[4], "LC_TIME=C");

        let lines = env(&[("LANG", "en_US.UTF-8"), ("LC_TIME", "C"), ("LC_ALL", "C")]);
        assert_eq!(lines[4], "LC_TIME=\"C\"");
        assert_eq!(lines.last().map(String::as_str), Some("LC_ALL=C"));
    }

    #[test]
    fn windows_names_become_posix_ones() {
        assert_eq!(posix_name("en-US"), "en_US");
        assert_eq!(posix_name("nl-BE"), "nl_BE");
        assert_eq!(posix_name("sr-Latn-RS"), "sr_RS");
        assert_eq!(posix_name("en"), "en");
        assert_eq!(
            locales(["nl-BE".to_owned(), "en-US".to_owned(), "nl-BE".to_owned()]),
            ["C", "C.UTF-8", "POSIX", "en_US.UTF-8", "nl_BE.UTF-8"]
        );
    }

    #[test]
    fn the_command_line() {
        assert_eq!(parse(&args("")), Request::Environment);
        assert_eq!(parse(&args("-a")), Request::Locales);
        assert_eq!(parse(&args("--all-locales -v")), Request::Locales);
        assert_eq!(parse(&args("-m")), Request::Charmaps);
        assert_eq!(parse(&args("-uU")), Request::Windows(Which::UserUi, true));
        assert_eq!(parse(&args("-f")), Request::Windows(Which::Format, false));
        assert_eq!(
            parse(&args("-k LC_CTYPE")),
            Request::Definitions("-k".into())
        );
        assert_eq!(
            parse(&args("LC_CTYPE")),
            Request::Definitions("LC_CTYPE".into())
        );
        assert_eq!(parse(&args("--nope")), Request::Unknown("--nope".into()));
        assert_eq!(parse(&args("-V")), Request::Version);
    }
}
