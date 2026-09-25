# source: https://www.pement.org/sed/sed1line.txt (SPECIAL APPLICATIONS)
# desc: sort paragraphs of file alphabetically (portable {NL} marker version)
# tags: gnu-ext (\n in the final replacement)
printf 'zeta\nz2\n\nalpha\na2\n\nmid\n' > file
sed '/./{H;d;};x;s/\n/={NL}=/g' file | sort | sed '1s/={NL}=//;s/={NL}=/\n/g'
