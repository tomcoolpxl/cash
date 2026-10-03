# source: tests/sed-differential.sh, case classic-rev-characters, frozen into the corpus
# desc: classic-rev-characters
printf '%b' 'hello world\n12345\n' | sed '/\n/!G;s/\(.\)\(.*\n\)/&\2\1/;//D;s/.//'
