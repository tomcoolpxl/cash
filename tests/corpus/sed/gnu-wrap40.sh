# source: https://www.gnu.org/software/sed/manual/html_node/Line-length-adjustment.html
# desc: wrap lines at 40 characters (-E, \n in replacement)
# tags: gnu-ext (\n in replacement)
cat > wrap40.sed <<'EOF'
# outer loop
:x

# Append a newline followed by the next input line to the pattern buffer
N

# Remove all newlines from the pattern buffer
s/\n/ /g


# Inner loop
:y

# Add a newline after the first 40 characters
s/(.{40,40})/\1\n/

# If there is a newline in the pattern buffer
# (i.e. the previous substitution added a newline)
/\n/ {
    # There are newlines in the pattern buffer -
    # print the content until the first newline.
    P

   # Remove the printed characters and the first newline
   s/.*\n//

   # branch to label 'y' - repeat inner loop
   by
 }

# No newlines in the pattern buffer - Branch to label 'x' (outer loop)
# and read the next input line
bx
EOF
printf 'It was the best of times, it was\nthe worst of times, it\nwas the age of\nwisdom,\nit\nwas\nthe age\nof foolishness,\n' > two-cities-mix.txt
sed -E -f wrap40.sed two-cities-mix.txt
