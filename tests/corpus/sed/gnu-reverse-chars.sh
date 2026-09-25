# source: https://www.gnu.org/software/sed/manual/html_node/Reverse-chars-of-lines.html
# desc: reverse characters of lines (newline markers, tx before label)
cat > rev.sed <<'EOF'
/../! b

# Reverse a line.  Begin embedding the line between two newlines
s/^.*$/\
&\
/

# Move first character at the end.  The regexp matches until
# there are zero or one characters between the markers
tx
:x
s/\(\n.\)\(.*\)\(.\n\)/\3\2\1/
tx

# Remove the newline markers
s/\n//g
EOF
printf 'hello world\nab\nx\n\nabcdefg\n' | sed -f rev.sed
