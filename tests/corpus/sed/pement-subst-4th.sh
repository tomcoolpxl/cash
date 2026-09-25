# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: replace only the 4th instance of foo in a line
printf 'foo foo foo foo foo\nfoo foo\n' | sed 's/foo/bar/4'
