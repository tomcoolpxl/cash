# source: tests/sed-differential.sh, case range-inclusive, frozen into the corpus
# desc: range-inclusive
printf '%b' 'ignore\nSTART\ninside1\ninside2\nEND\nignore\n' | sed '-n' '/START/,/END/p'
