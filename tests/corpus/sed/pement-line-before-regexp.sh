# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the line immediately before a regexp
printf 'regexp first\nb\nc\nregexp d\ne\n' | sed -n '/regexp/{g;1!p;};h'
