# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete the last line of each paragraph
printf 'a1\na2\na3\n\nb1\nb2\n\nc1\n' | sed -n '/^$/{p;h;};/./{x;/./p;}'
