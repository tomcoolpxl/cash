# source: https://www.gnu.org/software/sed/manual/html_node/uniq-_002dd.html
# desc: print only duplicated lines (uniq -d)
cat > uniqd.sed <<'EOF'
$b
N
/^\(.*\)\n\1$/ {
    # Print the first of the duplicated lines
    s/.*\n//
    p

    # Loop until we get a different line
    :b
    $b
    N
    /^\(.*\)\n\1$/ {
        s/.*\n//
        bb
    }
}

# The last line cannot be followed by duplicates
$b

# Found a different one.  Leave it alone in the pattern space
# and go back to the top, hunting its duplicates
D
EOF
printf 'a\na\nb\nc\nc\nc\nd\ne\ne\n' | sed -n -f uniqd.sed
