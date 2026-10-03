# source: tests/sed-differential.sh, case double-space-lines, frozen into the corpus
# desc: double-space-lines
printf '%b' 'line1\nline2\nline3\n' | sed 'G'
