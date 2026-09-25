# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: grep, grep -v, field equality, field regex match
printf 'a b c d abc123 x afoo\nregex line two three four five six zed\n1 2 3 4 other 6 7\n' > f
awk '/regex/' f; echo --
awk '!/regex/' f; echo --
awk '$5 == "abc123"' f; echo --
awk '$5 != "abc123"' f; echo --
awk '!($5 == "abc123")' f; echo --
awk '$7  ~ /^[a-f]/' f; echo --
awk '$7 !~ /^[a-f]/' f
