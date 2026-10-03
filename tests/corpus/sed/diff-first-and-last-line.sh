# source: tests/sed-differential.sh, case first-and-last-line, frozen into the corpus
# desc: first-and-last-line
printf '%b' 'line1\nline2\nline3\nline4\n' | sed '-n' '1p;$p'
