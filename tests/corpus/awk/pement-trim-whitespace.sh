# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: delete leading, trailing, both whitespace; $1=$1 field rebuild
printf '  \t lead\ntrail \t \n  both   ways  \n' > f
awk '{sub(/^[ \t]+/, "")};1' f | sed -n l
awk '{sub(/[ \t]+$/, "")};1' f | sed -n l
awk '{gsub(/^[ \t]+|[ \t]+$/,"")};1' f | sed -n l
awk '{$1=$1};1' f | sed -n l
