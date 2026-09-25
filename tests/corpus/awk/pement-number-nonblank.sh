# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: number each non-blank line, two methods
printf 'a\n\nb\n  \nc\n' > f
awk 'NF{$0=++a " :" $0};1' f; echo --
awk '{print (NF? ++a " :" :"") $0}' f
