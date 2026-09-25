# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: if a line ends with a backslash, append the next line (getline var)
printf 'one \\\ntwo\nthree\nfour \\\n' > file1
awk '/\\$/ {sub(/\\$/,""); getline t; print $0 t; next}; 1' file*
