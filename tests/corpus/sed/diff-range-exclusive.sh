# source: tests/sed-differential.sh, case range-exclusive, frozen into the corpus
# desc: range-exclusive
printf '%b' 'ignore\nSTART\ninside1\ninside2\nEND\nignore\n' | sed '-n' '/START/,/END/{//!p;}'
