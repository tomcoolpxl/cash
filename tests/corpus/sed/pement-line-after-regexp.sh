# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the line immediately after a regexp
printf 'a\nregexp b\nc\nd\nregexp e\n' | sed -n '/regexp/{n;p;}'
