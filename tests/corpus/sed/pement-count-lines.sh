# source: https://www.pement.org/sed/sed1line.txt (NUMBERING)
# desc: count lines (emulates "wc -l")
printf 'a\nb\nc\nd\n' | sed -n '$='
