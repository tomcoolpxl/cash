// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) ENOTDIR

use crate::messages::translate;
use clap::builder::ValueParser;
use clap::{Arg, ArgAction, Command};
use std::ffi::OsString;
use std::fs::{read_dir, remove_dir};
use std::io;
use std::path::Path;
use uucore::display::Quotable;
use uucore::error::{UResult, set_exit_code};

use uucore::{format_usage, show_error};

// cash: the tool's messages, compiled in (see `crate::messages`).
static MESSAGES: crate::messages::Messages =
    crate::messages::Messages::new(include_str!("en-US.ftl"));

static OPT_IGNORE_FAIL_NON_EMPTY: &str = "ignore-fail-on-non-empty";
static OPT_PARENTS: &str = "parents";
static OPT_VERBOSE: &str = "verbose";

static ARG_DIRS: &str = "dirs";

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uucore::clap_localization::handle_clap_result(uu_app(), args)?;

    let opts = Opts {
        ignore: matches.get_flag(OPT_IGNORE_FAIL_NON_EMPTY),
        parents: matches.get_flag(OPT_PARENTS),
        verbose: matches.get_flag(OPT_VERBOSE),
    };

    for path in matches
        .get_many::<OsString>(ARG_DIRS)
        .unwrap_or_default()
        .map(Path::new)
    {
        if let Err(error) = remove(path, opts) {
            let Error { error, path } = error;

            if opts.ignore && dir_not_empty(&error, path) {
                continue;
            }

            set_exit_code(1);

            // If `foo` is a symlink to a directory then `rmdir foo/` may give
            // a "not a directory" error. This is confusing as `rm foo/` says
            // "is a directory".
            // This differs from system to system. Some don't give an error.
            // Windows simply allows calling RemoveDirectory on symlinks so we
            // don't need to worry about it here.
            // GNU rmdir seems to print "Symbolic link not followed" if:
            // - It has a trailing slash
            // - It's a symlink
            // - It either points to a directory or dangles

            show_error!(
                "{}",
                translate!("rmdir-error-failed-to-remove", "path" => path.quote(), "err" => cash_win32::errno::strerror(&error))
            );
        }
    }

    Ok(())
}

struct Error<'a> {
    error: io::Error,
    path: &'a Path,
}

fn remove(mut path: &Path, opts: Opts) -> Result<(), Error<'_>> {
    remove_single(path, opts)?;
    if opts.parents {
        while let Some(new) = path.parent() {
            path = new;
            if path.as_os_str().is_empty() {
                break;
            }
            remove_single(path, opts)?;
        }
    }
    Ok(())
}

fn remove_single(path: &Path, opts: Opts) -> Result<(), Error<'_>> {
    if opts.verbose {
        println!(
            "{}",
            translate!("rmdir-verbose-removing-directory", "util_name" => "rmdir", "path" => path.quote())
        );
    }
    remove_dir_even_read_only(path).map_err(|error| Error { error, path })
}

/// cash: removes an empty directory, its read-only attribute notwithstanding. Windows
/// refuses to remove a directory that carries the attribute, which Explorer sets to mark a
/// customised directory and which bars no write into it; the attribute is taken off and
/// the removal asked again, as GNU rmdir takes a directory whatever its mode.
#[expect(
    clippy::permissions_set_readonly_false,
    reason = "the attribute is the one thing in the way; Windows has no world-writable bit"
)]
fn remove_dir_even_read_only(path: &Path) -> std::io::Result<()> {
    match remove_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let Ok(metadata) = std::fs::symlink_metadata(path) else {
                return Err(error);
            };
            if !metadata.permissions().readonly() {
                return Err(error);
            }
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions)?;
            remove_dir(path)
        }
        result => result,
    }
}

// POSIX: https://pubs.opengroup.org/onlinepubs/009696799/functions/rmdir.html

// 145 is ERROR_DIR_NOT_EMPTY, determined experimentally.
const NOT_EMPTY_CODES: &[i32] = &[145];

// Other error codes you might get for directories that could be found and are
// not empty.
// This is a subset of the error codes listed in rmdir(2) from the Linux man-pages
// project. Maybe other systems have additional codes that apply?

// Probably incomplete, I can't find a list of possible errors for
// RemoveDirectory anywhere.
const PERHAPS_EMPTY_CODES: &[i32] = &[
    5, // ERROR_ACCESS_DENIED, found experimentally.
];

fn dir_not_empty(error: &io::Error, path: &Path) -> bool {
    error.raw_os_error().is_some_and(|code| {
        NOT_EMPTY_CODES.contains(&code)
        // If --ignore-fail-on-non-empty is used then we want to ignore all errors
        // for non-empty directories, even if the error was e.g. because there's
        // no permission. So we do an additional check.
            || PERHAPS_EMPTY_CODES.contains(&code)
                && read_dir(path).is_ok_and(|mut iter| iter.next().is_some())
    })
}

#[derive(Clone, Copy, Debug)]
struct Opts {
    ignore: bool,
    parents: bool,
    verbose: bool,
}

pub fn uu_app() -> Command {
    Command::new("rmdir")
        .version(crate::UUTILS_VERSION)
        .help_template(uucore::localized_help_template("rmdir"))
        .about(translate!("rmdir-about"))
        .override_usage(format_usage(&translate!("rmdir-usage")))
        .infer_long_args(true)
        .arg(
            Arg::new(OPT_IGNORE_FAIL_NON_EMPTY)
                .long(OPT_IGNORE_FAIL_NON_EMPTY)
                .help(translate!("rmdir-help-ignore-fail-non-empty"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_PARENTS)
                .short('p')
                .long(OPT_PARENTS)
                .help(translate!("rmdir-help-parents"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_VERBOSE)
                .short('v')
                .long(OPT_VERBOSE)
                .help(translate!("rmdir-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(ARG_DIRS)
                .action(ArgAction::Append)
                .num_args(1..)
                .required(true)
                .value_parser(ValueParser::os_string())
                .value_hint(clap::ValueHint::DirPath),
        )
}
