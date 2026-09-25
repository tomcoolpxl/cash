# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print 1 line of context before and after regexp, with line number
printf 'a\nb\nregexp c\nd\ne\nregexp f\n' | sed -n -e '/regexp/{=;x;1!p;g;$!N;p;D;}' -e h
