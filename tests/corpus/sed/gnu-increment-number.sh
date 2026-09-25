# source: https://www.gnu.org/software/sed/manual/html_node/Increment-a-number.html
# desc: increment a number (Bruno Haible's single-buffer algorithm)
cat > incr.sed <<'EOF'
/[^0-9]/ d

# replace all trailing 9s by _ (any other character except digits, could
# be used)
:d
s/9\(_*\)$/_\1/
td

# incr last digit only.  The first line adds a most-significant
# digit of 1 if we have to add a digit.

s/^\(_*\)$/1\1/; tn
s/8\(_*\)$/9\1/; tn
s/7\(_*\)$/8\1/; tn
s/6\(_*\)$/7\1/; tn
s/5\(_*\)$/6\1/; tn
s/4\(_*\)$/5\1/; tn
s/3\(_*\)$/4\1/; tn
s/2\(_*\)$/3\1/; tn
s/1\(_*\)$/2\1/; tn
s/0\(_*\)$/1\1/; tn

:n
y/_/0/
EOF
printf '0\n9\n199\n1234\n999\nabc\n' | sed -f incr.sed
