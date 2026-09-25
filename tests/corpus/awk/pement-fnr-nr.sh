# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: precede each line by its line number FOR THAT FILE and FOR ALL FILES
printf 'a\nb\n' > files1; printf 'c\nd\ne\n' > files2
awk '{print FNR "\t" $0}' files*
awk '{print NR "\t" $0}' files*
