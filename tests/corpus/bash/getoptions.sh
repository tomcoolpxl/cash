# source: https://github.com/ko1nksm/getoptions (lib/getoptions_base.sh, _abbr.sh and _help.sh as fixtures, unchanged; the definition is examples/basic.sh's)
# desc: a real option-parser generator: shell code built as text and eval'd, its quote(), abbreviations, help through a quoted heredoc
. ./getoptions_base.sh
. ./getoptions_abbr.sh
. ./getoptions_help.sh
VERSION=0.1
PROG=basic

# shellcheck disable=SC1083
parser_definition() {
	setup   REST plus:true help:usage abbr:true -- \
		"Usage: $PROG [options...] [arguments...]" ''
	msg -- 'getoptions basic example' ''
	msg -- 'Options:'
	flag    FLAG_A  -a                                        -- "message a"
	flag    FLAG_B  -b                                        -- "message b"
	flag    FLAG_F  -f +f --{no-}flag                         -- "expands to --flag and --no-flag"
	flag    FLAG_W        --with{out}-flag                    -- "expands to ---with-flag and --without-flag"
	flag    VERBOSE -v    --verbose   counter:true init:=0    -- "e.g. -vvv is verbose level 3"
	param   PARAM   -p    --param     pattern:"foo | bar"     -- "accepts --param value / --param=value"
	param   NUMBER  -n    --number    validate:number         -- "accepts only a number value"
	option  OPTION  -o    --option    on:"default"            -- "accepts -ovalue / --option=value"
	disp    :usage  -h    --help
	disp    VERSION       --version
}

number() { case $OPTARG in (*[!0-9]*) return 1; esac; }

run() {
	(
		set -eu
		eval "$(getoptions parser_definition) exit 1"
		printf 'a=%s b=%s f=%s w=%s v=%s p=%s n=%s o=%s |' "$FLAG_A" "$FLAG_B" "$FLAG_F" \
			"$FLAG_W" "$VERBOSE" "$PARAM" "$NUMBER" "$OPTION"
		printf ' [%s]' "$@"
		printf '\n'
	) 2>&1
	echo "  status $?"
}
run
run -a -b x y
run -ab -vvv --flag
run +f --no-flag --with-flag --without-flag
run -p foo --param=bar -n 42 -ovalue --option
run --option=given --number 7 -- -a 'two words'
run --param baz
run -n abc
run -x
run --flag=1
run -p
run --verb --para foo
run --ver
run a -- b
run --help
run --version
echo "-- the generated parser"
getoptions parser_definition parse
