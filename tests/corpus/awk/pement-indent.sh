# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: insert 5 blank spaces at beginning of each line (sub on /^/)
printf 'a\nb\n' | awk '{sub(/^/, "     ")};1'
