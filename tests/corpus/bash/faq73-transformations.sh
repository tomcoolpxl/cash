# source: https://mywiki.wooledge.org/BashFAQ/073 (Bash 4 / parameter transformation)
# desc: ^ ^^ , ,, and @Q @E @A @a transformations
string='hello, World!'
echo "${string^}"; echo "${string^^}"; echo "${string,}"; echo "${string,,}"
string=$'nice "day" isn\'t it?' ; echo "${string@Q}"
string='hello\tworld' ; echo "${string@E}"
string=hello ; echo "${string@A}"
a=(an array); echo "${a[@]@A}"
declare -ri i=3 ; echo "${i@A}"
echo "${string@a}"
echo "${a@a}" "${i@a}"
