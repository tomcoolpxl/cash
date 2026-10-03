//! The synopsis Bash 5.3 gives each of its builtins: what `help -s` prints, and how a
//! usage error ends (`read: usage: read [-Eers] [-a array] …`). Taken from Git Bash 5.3's
//! `help -s` for every name `compgen -b` lists.

/// Bash's synopsis of the builtin `name`, if Bash has one by that name.
pub(crate) fn synopsis(name: &str) -> Option<&'static str> {
    SYNOPSES
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, synopsis)| *synopsis)
}

const SYNOPSES: &[(&str, &str)] = &[
    (".", ". [-p path] filename [arguments]"),
    (":", ":"),
    ("[", "[ arg... ]"),
    ("alias", "alias [-p] [name[=value] ... ]"),
    ("bg", "bg [job_spec ...]"),
    (
        "bind",
        "bind [-lpsvPSVX] [-m keymap] [-f filename] [-q name] [-u name] [-r keyseq] [-x keyseq:shell-command] [keyseq:readline-function or readline-command]",
    ),
    ("break", "break [n]"),
    ("builtin", "builtin [shell-builtin [arg ...]]"),
    ("caller", "caller [expr]"),
    ("cd", "cd [-L|[-P [-e]]] [-@] [dir]"),
    ("command", "command [-pVv] command [arg ...]"),
    (
        "compgen",
        "compgen [-V varname] [-abcdefgjksuv] [-o option] [-A action] [-G globpat] [-W wordlist] [-F function] [-C command] [-X filterpat] [-P prefix] [-S suffix] [word]",
    ),
    (
        "complete",
        "complete [-abcdefgjksuv] [-pr] [-DEI] [-o option] [-A action] [-G globpat] [-W wordlist] [-F function] [-C command] [-X filterpat] [-P prefix] [-S suffix] [name ...]",
    ),
    ("compopt", "compopt [-o|+o option] [-DEI] [name ...]"),
    ("continue", "continue [n]"),
    (
        "declare",
        "declare [-aAfFgiIlnrtux] [name[=value] ...] or declare -p [-aAfFilnrtux] [name ...]",
    ),
    ("dirs", "dirs [-clpv] [+N] [-N]"),
    ("disown", "disown [-h] [-ar] [jobspec ... | pid ...]"),
    ("echo", "echo [-neE] [arg ...]"),
    ("enable", "enable [-a] [-dnps] [-f filename] [name ...]"),
    ("eval", "eval [arg ...]"),
    (
        "exec",
        "exec [-cl] [-a name] [command [argument ...]] [redirection ...]",
    ),
    ("exit", "exit [n]"),
    (
        "export",
        "export [-fn] [name[=value] ...] or export -p [-f]",
    ),
    ("false", "false"),
    (
        "fc",
        "fc [-e ename] [-lnr] [first] [last] or fc -s [pat=rep] [command]",
    ),
    ("fg", "fg [job_spec]"),
    ("getopts", "getopts optstring name [arg ...]"),
    ("hash", "hash [-lr] [-p pathname] [-dt] [name ...]"),
    ("help", "help [-dms] [pattern ...]"),
    (
        "history",
        "history [-c] [-d offset] [n] or history -anrw [filename] or history -ps arg [arg...]",
    ),
    (
        "jobs",
        "jobs [-lnprs] [jobspec ...] or jobs -x command [args]",
    ),
    (
        "kill",
        "kill [-s sigspec | -n signum | -sigspec] pid | jobspec ... or kill -l [sigspec]",
    ),
    ("let", "let arg [arg ...]"),
    ("local", "local [option] name[=value] ..."),
    ("logout", "logout [n]"),
    (
        "mapfile",
        "mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]",
    ),
    ("popd", "popd [-n] [+N | -N]"),
    ("printf", "printf [-v var] format [arguments]"),
    ("pushd", "pushd [-n] [+N | -N | dir]"),
    ("pwd", "pwd [-LPW]"),
    (
        "read",
        "read [-Eers] [-a array] [-d delim] [-i text] [-n nchars] [-N nchars] [-p prompt] [-t timeout] [-u fd] [name ...]",
    ),
    (
        "readarray",
        "readarray [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]",
    ),
    (
        "readonly",
        "readonly [-aAf] [name[=value] ...] or readonly -p",
    ),
    ("return", "return [n]"),
    (
        "set",
        "set [-abefhkmnptuvxBCEHPT] [-o option-name] [--] [-] [arg ...]",
    ),
    ("shift", "shift [n]"),
    ("shopt", "shopt [-pqsu] [-o] [optname ...]"),
    ("source", "source [-p path] filename [arguments]"),
    ("suspend", "suspend [-f]"),
    ("test", "test [expr]"),
    ("times", "times"),
    ("trap", "trap [-Plp] [[action] signal_spec ...]"),
    ("true", "true"),
    ("type", "type [-afptP] name [name ...]"),
    (
        "typeset",
        "typeset [-aAfFgiIlnrtux] name[=value] ... or typeset -p [-aAfFilnrtux] [name ...]",
    ),
    ("ulimit", "ulimit [-SHabcdefiklmnpqrstuvxPRT] [limit]"),
    ("umask", "umask [-p] [-S] [mode]"),
    ("unalias", "unalias [-a] name [name ...]"),
    ("unset", "unset [-f] [-v] [-n] [name ...]"),
    ("wait", "wait [-fn] [-p var] [id ...]"),
];
