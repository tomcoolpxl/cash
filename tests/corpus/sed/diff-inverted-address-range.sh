# source: tests/sed-differential.sh, case inverted-address-range, frozen into the corpus
# desc: inverted-address-range
printf '%b' 'one\ntwo\nthree\nfour\nfive\n' | sed '2,3!d'
