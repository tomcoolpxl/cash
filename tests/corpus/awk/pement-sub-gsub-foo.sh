# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: substitute foo with bar: first only, all, only on baz lines, except baz lines
printf 'foo foo baz foo\nfoo x foo\n' > f
awk '{sub(/foo/,"bar")}; 1' f
awk '{gsub(/foo/,"bar")}; 1' f
awk '/baz/{gsub(/foo/, "bar")}; 1' f
awk '!/baz/{gsub(/foo/, "bar")}; 1' f
