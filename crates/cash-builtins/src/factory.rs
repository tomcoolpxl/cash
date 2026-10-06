use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use cash_core::builtins::{self, builtin, decl_builtin, raw_arg_builtin, simple_builtin};

/// Identifies well-known sets of builtins.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BuiltinSet {
    /// Identifies builtins appropriate for POSIX `sh` compatibility.
    ShMode,
    /// Identifies builtins appropriate for a more full-featured `bash`-compatible shell.
    BashMode,
}

/// Returns the default set of built-in commands.
///
/// # Arguments
///
/// * `set` - The set of built-ins to return.
#[expect(clippy::too_many_lines)]
pub fn default_builtins<SE: cash_core::ShellExtensions>(
    set: BuiltinSet,
) -> HashMap<String, builtins::Registration<SE>> {
    let mut m = HashMap::<String, builtins::Registration<SE>>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    m.insert(
        "break".into(),
        builtin::<break_::BreakCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(
        ":".into(),
        simple_builtin::<colon::ColonCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        builtin::<continue_::ContinueCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(
        ".".into(),
        builtin::<dot::DotCommand, SE>()
            .special()
            .with_substitution_files(),
    );
    #[cfg(feature = "builtin.eval")]
    m.insert(
        "eval".into(),
        builtin::<eval::EvalCommand, SE>()
            .special()
            .with_substitution_files(),
    );
    #[cfg(feature = "builtin.exec")]
    m.insert(
        "exec".into(),
        builtin::<exec::ExecCommand, SE>()
            .special()
            .with_substitution_files(),
    );
    #[cfg(feature = "builtin.exit")]
    m.insert("exit".into(), builtin::<exit::ExitCommand, SE>().special());
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        decl_builtin::<export::ExportCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        builtin::<return_::ReturnCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert("set".into(), builtin::<set::SetCommand, SE>().special());
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        builtin::<shift::ShiftCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert("trap".into(), builtin::<trap::TrapCommand, SE>().special());
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        builtin::<unset::UnsetCommand, SE>().special(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        decl_builtin::<declare::DeclareCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        builtin::<times::TimesCommand, SE>().special(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert("alias".into(), builtin::<alias::AliasCommand, SE>()); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), builtin::<bg::BgCommand, SE>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), builtin::<cd::CdCommand, SE>());
    #[cfg(feature = "builtin.command")]
    m.insert(
        "command".into(),
        builtin::<command::CommandCommand, SE>().with_substitution_files(),
    );
    #[cfg(feature = "builtin.false")]
    m.insert("false".into(), simple_builtin::<false_::FalseCommand, SE>());
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), builtin::<fg::FgCommand, SE>());
    #[cfg(feature = "builtin.getopts")]
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand, SE>());
    #[cfg(feature = "builtin.hash")]
    m.insert("hash".into(), builtin::<hash::HashCommand, SE>());
    #[cfg(feature = "builtin.help")]
    m.insert("help".into(), builtin::<help::HelpCommand, SE>());
    #[cfg(feature = "builtin.jobs")]
    m.insert("jobs".into(), builtin::<jobs::JobsCommand, SE>());
    #[cfg(feature = "builtin.kill")]
    m.insert("kill".into(), builtin::<kill::KillCommand, SE>());
    #[cfg(feature = "builtin.declare")]
    m.insert(
        "local".into(),
        decl_builtin::<declare::DeclareCommand, SE>(),
    );
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), builtin::<pwd::PwdCommand, SE>());
    #[cfg(feature = "builtin.read")]
    m.insert("read".into(), builtin::<read::ReadCommand, SE>());
    #[cfg(feature = "builtin.true")]
    m.insert("true".into(), simple_builtin::<true_::TrueCommand, SE>());
    #[cfg(feature = "builtin.type")]
    m.insert("type".into(), builtin::<type_::TypeCommand, SE>());
    #[cfg(feature = "builtin.ulimit")]
    m.insert("ulimit".into(), builtin::<ulimit_win::UlimitCommand, SE>());
    #[cfg(feature = "builtin.umask")]
    m.insert("umask".into(), builtin::<umask::UmaskCommand, SE>());
    #[cfg(feature = "builtin.unalias")]
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand, SE>());
    #[cfg(feature = "builtin.wait")]
    m.insert("wait".into(), builtin::<wait::WaitCommand, SE>());

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), builtin::<fc::FcCommand, SE>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            raw_arg_builtin::<builtin_::BuiltinCommand, SE>().with_substitution_files(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "declare".into(),
            decl_builtin::<declare::DeclareCommand, SE>(),
        );
        #[cfg(feature = "builtin.echo")]
        m.insert("echo".into(), builtin::<echo::EchoCommand, SE>());
        #[cfg(feature = "builtin.enable")]
        m.insert("enable".into(), builtin::<enable::EnableCommand, SE>());
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), builtin::<let_::LetCommand, SE>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("mapfile".into(), builtin::<mapfile::MapFileCommand, SE>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("readarray".into(), builtin::<mapfile::MapFileCommand, SE>());
        #[cfg(feature = "builtin.printf")]
        m.insert("printf".into(), builtin::<printf::PrintfCommand, SE>());
        #[cfg(feature = "builtin.shopt")]
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand, SE>());
        #[cfg(feature = "builtin.dot")]
        m.insert(
            "source".into(),
            builtin::<dot::DotCommand, SE>()
                .special()
                .with_substitution_files(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert("test".into(), builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "typeset".into(),
            decl_builtin::<declare::DeclareCommand, SE>(),
        );

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert(
            "complete".into(),
            builtin::<complete::CompleteCommand, SE>(),
        );
        #[cfg(feature = "builtin.compgen")]
        m.insert("compgen".into(), builtin::<complete::CompGenCommand, SE>());
        #[cfg(feature = "builtin.compopt")]
        m.insert("compopt".into(), builtin::<complete::CompOptCommand, SE>());

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert("dirs".into(), builtin::<dirs::DirsCommand, SE>());
        #[cfg(feature = "builtin.popd")]
        m.insert("popd".into(), builtin::<popd::PopdCommand, SE>());
        #[cfg(feature = "builtin.pushd")]
        m.insert("pushd".into(), builtin::<pushd::PushdCommand, SE>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert("bind".into(), builtin::<bind::BindCommand, SE>());

        // History
        #[cfg(feature = "builtin.history")]
        m.insert("history".into(), builtin::<history::HistoryCommand, SE>());

        #[cfg(feature = "builtin.caller")]
        m.insert("caller".into(), builtin::<caller::CallerCommand, SE>());

        #[cfg(feature = "builtin.disown")]
        m.insert("disown".into(), builtin::<disown::DisownCommand, SE>());

        m.insert("logout".into(), builtin::<logout::LogoutCommand, SE>());

        // cash (D60): fish's abbreviations, expanded by the interactive line editor.
        m.insert("abbr".into(), builtin::<abbr::AbbrCommand, SE>());

        // cash (D62): fish's folder history, also on Alt-← and Alt-→ at the prompt.
        m.insert("prevd".into(), builtin::<dirhistory::PrevdCommand, SE>());
        m.insert("nextd".into(), builtin::<dirhistory::NextdCommand, SE>());
        m.insert("cdh".into(), builtin::<dirhistory::CdhCommand, SE>());

        // cash (D73): the file and folder picker, also on Alt-E at the prompt.
        m.insert("croot".into(), builtin::<croot::CrootCommand, SE>());
    }

    // cash (D45): Windows-specific builtins. Not feature-gated per builtin — each is the
    // documented escape hatch for a decision made elsewhere (D4, D6, D42).
    m.insert("winpath".into(), builtin::<win::WinPathCommand, SE>());
    m.insert(
        "start".into(),
        builtin::<win::StartCommand, SE>().with_substitution_files(),
    );
        "xdg-open".into(),
        builtin::<xdg_open::XdgOpenCommand, SE>().with_substitution_files(),
    m.insert("pbcopy".into(), builtin::<pbcopy::PbcopyCommand, SE>());
    m.insert("pbpaste".into(), builtin::<pbcopy::PbpasteCommand, SE>());
    m.insert(
        "elevate".into(),
        builtin::<win::ElevateCommand, SE>().with_substitution_files(),
    );
    m.insert(
        "detach".into(),
        builtin::<win::DetachCommand, SE>().with_substitution_files(),
    );
    m.insert(
        "sudo".into(),
        builtin::<win::SudoCommand, SE>().with_substitution_files(),
    );
    m.insert("su".into(), builtin::<win::SuCommand, SE>());
    m.insert("sudoedit".into(), builtin::<win::SudoeditCommand, SE>());
    m.insert("ps".into(), builtin::<ps::PsCommand, SE>());
    m.insert("pgrep".into(), builtin::<pgrep::PgrepCommand, SE>());
    m.insert("pkill".into(), builtin::<killfam::PkillCommand, SE>());
    m.insert("free".into(), builtin::<free::FreeCommand, SE>());
        "nice".into(),
        builtin::<nice::NiceCommand, SE>().with_substitution_files(),
    m.insert("renice".into(), builtin::<nice::ReniceCommand, SE>());
    m.insert("pidof".into(), builtin::<killfam::PidofCommand, SE>());
    m.insert("killall".into(), builtin::<killfam::KillallCommand, SE>());
    m.insert("getopt".into(), builtin::<getopt::GetoptCommand, SE>());
    m.insert("rev".into(), builtin::<rev::RevCommand, SE>());
    m.insert("clear".into(), builtin::<screen::ClearCommand, SE>());
    m.insert("reset".into(), builtin::<screen::ResetCommand, SE>());
    m.insert("fuser".into(), builtin::<fuser::FuserCommand, SE>());
        "flock".into(),
        builtin::<flock::FlockCommand, SE>().with_substitution_files(),
    m.insert("lsof".into(), builtin::<lsof::LsofCommand, SE>());
    m.insert("ss".into(), builtin::<ss::SsCommand, SE>());
    m.insert("pstree".into(), builtin::<pstree::PsTreeCommand, SE>());
    m.insert("tree".into(), builtin::<tree::TreeCommand, SE>());
    m.insert("top".into(), builtin::<top::TopCommand, SE>());
    m.insert(
        "find".into(),
        builtin::<find::FindCommand, SE>().with_substitution_files(),
    );
    m.insert("uuidgen".into(), builtin::<uuidgen::UuidgenCommand, SE>());
    m.insert(
        "xargs".into(),
        builtin::<xargs::XargsCommand, SE>().with_substitution_files(),
    );
    m.insert("nc".into(), builtin::<nc::NcCommand, SE>());
    m.insert("xxd".into(), builtin::<xxd::XxdCommand, SE>());
    m.insert("hexdump".into(), builtin::<hexdump::HexdumpCommand, SE>());
    m.insert("column".into(), builtin::<column::ColumnCommand, SE>());
    m.insert(
        "coolfetch".into(),
        builtin::<coolfetch::CoolfetchCommand, SE>(),
    );
    m.insert(
        "hostname".into(),
    m.insert("tput".into(), builtin::<tput::TputCommand, SE>());
    m.insert("watch".into(), builtin::<watch::WatchCommand, SE>());
    m.insert("stty".into(), builtin::<stty::SttyCommand, SE>());
        builtin::<hostname::HostnameCommand, SE>(),
    );
    m.insert("less".into(), builtin::<pager::LessCommand, SE>());
    m.insert("more".into(), builtin::<pager::MoreCommand, SE>());
    m.insert("which".into(), builtin::<which::WhichCommand, SE>());
    m.insert("where".into(), builtin::<where_files::WhereCommand, SE>());
    m.insert("chmod".into(), builtin::<chmod::ChmodCommand, SE>());
    m.insert("id".into(), builtin::<identity::IdCommand, SE>());
    m.insert("groups".into(), builtin::<identity::GroupsCommand, SE>());
    m.insert("logname".into(), builtin::<identity::LognameCommand, SE>());
    m.insert("hostid".into(), builtin::<identity::HostidCommand, SE>());
    m.insert("users".into(), builtin::<identity::UsersCommand, SE>());
    m.insert("who".into(), builtin::<identity::WhoCommand, SE>());
    m.insert("pinky".into(), builtin::<identity::PinkyCommand, SE>());
    m.insert("ls".into(), builtin::<ls::LsCommand, SE>());
    m.insert("tty".into(), builtin::<tty::TtyCommand, SE>());
    m.insert("stat".into(), builtin::<stat::StatCommand, SE>());
    m.insert(
        "nohup".into(),
        builtin::<nohup::NohupCommand, SE>().with_substitution_files(),
    );
    m.insert(
        "install".into(),
        builtin::<install::InstallCommand, SE>().with_substitution_files(),
    );
    m.insert(
        "dos2unix".into(),
        builtin::<dos2unix::Dos2UnixCommand, SE>().with_substitution_files(),
    );
    m.insert(
        "unix2dos".into(),
        builtin::<dos2unix::Unix2DosCommand, SE>().with_substitution_files(),
    );

    m
}
        "iconv".into(),
        builtin::<iconv::IconvCommand, SE>().with_substitution_files(),
