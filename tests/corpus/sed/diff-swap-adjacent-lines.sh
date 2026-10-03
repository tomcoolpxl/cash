# source: tests/sed-differential.sh, case swap-adjacent-lines, frozen into the corpus
# desc: swap-adjacent-lines
printf '%b' 'line1\nline2\nline3\nline4\n' | sed '-n' 'h;n;G;p'
