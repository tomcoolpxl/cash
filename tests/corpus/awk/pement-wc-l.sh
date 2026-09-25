# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: count lines (emulates wc -l)
printf 'a\nb\nc\n' | awk 'END{print NR}'
