# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: replace the next-to-last case of foo
printf 'foo1 foo2 foo3 foo4\n' | sed 's/\(.*\)foo\(.*foo\)/\1bar\2/'
