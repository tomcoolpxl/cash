# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE DELETION)
# desc: remove duplicate, nonconsecutive lines (!a[$0]++ and "in")
printf 'b\na\nb\nc\na\n\n\nd\n' > f
awk '!a[$0]++' f; echo --
awk '!($0 in a){a[$0];print}' f
