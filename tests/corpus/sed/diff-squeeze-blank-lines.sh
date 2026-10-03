# source: tests/sed-differential.sh, case squeeze-blank-lines, frozen into the corpus
# desc: squeeze-blank-lines
printf '%b' 'line1\n\n\n\nline2\n\nline3\n' | sed '/^$/{N;/^\n$/D;}'
