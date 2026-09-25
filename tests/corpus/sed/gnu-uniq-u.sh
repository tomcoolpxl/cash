# source: https://www.gnu.org/software/sed/manual/html_node/uniq-_002du.html
# desc: print only unique lines (uniq -u)
cat > uniqu.sed <<'EOF'
# Search for a duplicate line --- until that, print what you find.
$b
N
/^\(.*\)\n\1$/ ! {
    P
    D
}

:c
# Got two equal lines in pattern space.  At the
# end of the file we simply exit
$d

# Else, we keep reading lines with N until we
# find a different one
s/.*\n//
N
/^\(.*\)\n\1$/ {
    bc
}

# Remove the last instance of the duplicate line
# and go back to the top
D
EOF
printf 'a\na\nb\nc\nc\nc\nd\ne\n' | sed -f uniqu.sed
