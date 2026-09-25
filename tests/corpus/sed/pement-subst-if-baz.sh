# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: substitute foo with bar ONLY / EXCEPT for lines containing baz
printf 'foo baz foo\nfoo qux foo\n' > in.txt
sed '/baz/s/foo/bar/g' in.txt
sed '/baz/!s/foo/bar/g' in.txt
