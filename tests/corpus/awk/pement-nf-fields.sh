# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: NF:$0, last field, last field of last line, NF>4, $NF>4
printf 'a b c\none\n1 2 3 4 5\nx y 10\n\n' > f
awk '{ print NF ":" $0 } ' f; echo --
awk '{ print $NF }' f; echo --
awk '{ field = $NF }; END{ print field }' f; echo --
awk 'NF > 4' f; echo --
awk '$NF > 4' f
