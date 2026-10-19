# source: https://github.com/kward/shflags/blob/master/shflags (the fixture `shflags`, unchanged)
# desc: a real library sourced: DEFINE_ flags of four kinds, enhanced getopt found by getopt -T, long and short flags, --noflag, eval set -- "${FLAGS_ARGV}", a bad value's status, the help
# timeout: 30
run() {
  ( . ./shflags
    DEFINE_string 'name' 'world' 'name to greet' 'n'
    DEFINE_boolean 'verbose' false 'say more' 'v'
    DEFINE_integer 'count' 1 'how many times' 'c'
    DEFINE_float 'ratio' 0.5 'a ratio' 'r'
    FLAGS "$@" || exit $?
    eval set -- "${FLAGS_ARGV}"
    i=0
    while [ "$i" -lt "${FLAGS_count}" ]; do
      echo "Hello, ${FLAGS_name}!"
      i=$((i + 1))
    done
    if [ "${FLAGS_verbose}" -eq "${FLAGS_TRUE}" ]; then
      echo "verbose: ratio ${FLAGS_ratio}, rest: $*"
    fi
  )
}
run -n cash --noverbose x
run --name=bash --count 2 -v -r 0.25 one two
run --count notanumber 2>/dev/null; echo "bad count: $?"
# the usage names $0, the shell itself under -c
run --help 2>&1 | sed 's|^USAGE: .* \[flags\]|USAGE: NAME [flags]|'; echo "help: ${PIPESTATUS[0]}"
