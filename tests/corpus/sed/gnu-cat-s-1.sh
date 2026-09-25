# source: https://www.gnu.org/software/sed/manual/html_node/cat-_002ds.html
# desc: squeeze blank lines, first script (newline in replacement)
cat > cats1.sed <<'EOF'
# on empty lines, join with next
# Note there is a star in the regexp
:x
/^\n*$/ {
N
bx
}

# now, squeeze all '\n', this can be also done by:
# s/^\(\n\)*/\1/
s/\n*/\
/
EOF
printf '\n\na\n\n\nb\nc\n\n' | sed -f cats1.sed | sed -n l
