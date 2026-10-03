# source: tests/sed-differential.sh, case subst-3rd-occurrence, frozen into the corpus
# desc: subst-3rd-occurrence
printf '%b' 'foo foo foo foo foo\n' | sed 's/foo/BAR/3'
