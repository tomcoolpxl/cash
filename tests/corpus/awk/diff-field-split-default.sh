# source: tests/awk-differential.sh, case field-split-default, frozen into the corpus
# desc: field-split-default
printf '%b' 'foo bar baz\n' | awk '{ print $1, $3 }'
