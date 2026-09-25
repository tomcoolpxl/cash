# source: https://www.pement.org/awk/awk1line.txt (STRING CREATION)
# desc: create a string of a specific length (513 spaces)
awk 'BEGIN{while (a++<513) s=s " "; print s}' | awk '{print length($0)}'
