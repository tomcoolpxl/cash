# source: tests/sed-differential.sh, case classic-join-pairs, frozen into the corpus
# desc: classic-join-pairs
printf '%b' 'one\ntwo\nthree\nfour\n' | sed '$!N;s/\n/ /'
