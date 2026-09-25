# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete duplicate, nonconsecutive lines
printf 'b\na\nb\nc\na\nd\n' | sed -n 'G; s/\n/&&/; /^\([ -~]*\n\).*\n\1/d; s/\n//; h; P'
