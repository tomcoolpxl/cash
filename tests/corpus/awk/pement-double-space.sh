# source: https://www.pement.org/awk/awk1line.txt (FILE SPACING)
# desc: double space a file, two methods; skip-blank variant; triple space
printf 'a\n\nb\n' > f
awk '1;{print ""}' f; echo --
awk 'BEGIN{ORS="\n\n"};1' f; echo --
awk 'NF{print $0 "\n"}' f; echo --
awk '1;{print "\n"}' f
