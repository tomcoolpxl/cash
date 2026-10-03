# source: tests/sed-differential.sh, case classic-uniq-duplicate-lines, frozen into the corpus
# desc: classic-uniq-duplicate-lines
printf '%b' 'a\na\nb\nc\nc\nc\nd\n' | sed '$!N; /^\(.*\)\n\1$/!P; D'
