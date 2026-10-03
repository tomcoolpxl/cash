# source: tests/sed-differential.sh, case join-all-lines, frozen into the corpus
# desc: join-all-lines
printf '%b' 'one\ntwo\nthree\nfour\n' | sed '-e' ':a' '-e' '$!N;s/\n/ /;ta'
