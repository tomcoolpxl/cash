# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: align flush right on 79 columns; center on 79 (dynamic printf width)
printf 'hello\nab cd\n' > file
awk '{printf "%79s\n", $0}' file*
awk '{l=length();s=int((79-l)/2); printf "%"(s+l)"s\n",$0}' file*
