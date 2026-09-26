# getopt cases, run under util-linux getopt (the oracle) and under cash's builtin.
# Each case prints its standard output, standard error and status, so the two runs can
# be compared byte for byte. getopt_cases.out is util-linux 2.42.3's output.
#
# Regenerate the golden file (WSL):
#   bash getopt_cases.sh > getopt_cases.out 2>&1

case_no=0
run() {
    case_no=$((case_no + 1))
    # getopt has no side effects, so it runs twice: once for its output and status,
    # once for its errors.
    out=$(getopt "$@" 2>/dev/null) ; rc=$?
    err=$(getopt "$@" 2>&1 >/dev/null)
    printf '## %d: getopt' "$case_no"
    printf ' [%s]' "$@"
    printf '\nout=[%s]\nerr=[%s]\nrc=%d\n' "$out" "$err" "$rc"
}

# Short options: flags, required and optional arguments, clusters.
run -o ab:c:: -- -a -b val file
run -o ab:c:: -- -abval -cx -c file
run -o ab: -- -b
run -o ab: -- -x file
run -o ab: -- file1 -a file2 -- -b notanoption
run -o ab: -- -a -- -b
run -o a -- 'it'\''s' 'two words' ''
run -o a -- "$(printf 'line1\nline2')" '$HOME' '`x`' '"q"' '\back'

# Long options.
run -o a -l alpha,beta:,gamma:: -- --alpha --beta=1 --beta 2 --gamma --gamma=3 x
run -o '' -l verbose,version -- --verb
run -o '' -l verbose,version -- --vers
run -o '' -l verbose,version -- --ver
run -o '' -l alpha -- --bogus
run -o '' -l alpha -- --alpha=1
run -o '' -l beta: -- --beta
run -o '' --longoptions=one,two -- --two --one
run -o '' -l one -l two -- --one --two

# Alternative long options with one dash.
run -a -o b -l long: -- -long x -b
run -o b -l long: -- -long x

# Stop at the first non-option ('+'), or return non-options in place ('-').
run -o +a -- file -a
run -o -a -- file -a
POSIXLY_CORRECT=1 run -o a -- file -a

# Naming, quiet, test-only, unquoted, and the shell style.
run -n myscript -o a -- -z
run -q -o a -- -z
run -Q -o a -- -a
run -Q -o a -- -z
run -u -o a: -- -a 'two words' 'x'
run -s sh -o a -- -a "it's"
run -s bash -o a -- -a "it's"
run -T
run -o a --name=other -- -z

# The traditional form: the first parameter is the option string.
run ab: -a -b val file
run ab: -x

# Edges of GNU option parsing.
run -o '' -l alpha -- --bogus=1
run -a -o b -l long: -- -xyz -long
run -a -o b -l long: -- -bx
run -o '' -l ver,verbose -- --ver
run -o a: -- -a -- -b
run -o '' -l beta: -- --beta -x
run -o :a -- -z -a
run -o '' -l 'one two' -- --one --two
run --options=ab -- -ab
run -qo a -- -z
run -o a -- -a --
run -o ab -- -a - -b
run -o ab -- -- --
run -o 'a:b' -- -a -b
run -o '+a' -- -a -- x

# Usage errors.
run
run -o
run -s zsh -o a -- -a
run -s tcsh -o a -- -a
run --bogus-getopt-option
