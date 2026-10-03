# source: tests/sed-differential.sh, case custom-delimiter-hash, frozen into the corpus
# desc: custom-delimiter-hash
printf '%b' 'http://example.com/test\n' | sed 's#http://#https://#g'
