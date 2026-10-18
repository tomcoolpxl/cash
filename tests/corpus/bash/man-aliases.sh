# source: https://www.gnu.org/software/bash/manual/bash.html#Aliases (cash's own case)
# desc: aliases read before parsing: operators in their text, a pipe after one, loops opened by one, function bodies, same-line definitions, and the words that are no command
shopt -s expand_aliases
alias both='echo first && echo second'
alias up='tr a-z A-Z'
alias say='echo said'
alias three='echo one; echo two; echo three'
both
both | up
three | up
alias loop3='for i in 1 2 3; do'
loop3 echo "i=$i"; done
alias ifyes='if true; then'
ifyes say inside; fi
f() { say in-f; }
f
alias say='echo changed'
f
say now
case say in say) echo "pattern not expanded";; esac
for w in say up; do echo "word $w"; done
cat <<say
say
say
echo hi > say; cat say; rm say
function g { say in-g; }
g
alias quote="echo 'a;b'"
quote
alias semi='echo semi;'
semi say after-semi
alias same='echo same'; same 2>/dev/null || echo "same line: not yet"
same
[[ say == say ]] && say brackets
x=1 say with-assignment
! say negated
{ say grouped; }
( say subshell )
echo $(say substituted)
if say cond; then say then; fi
alias inc='n=$((n + 1))'
n=0; inc; inc; echo "n=$n"
alias a1='a2 ' a2='echo a2' a3='say'
a1 a3 chained
\say quoted 2>/dev/null || echo "quoted is no alias"
unalias say
say gone 2>/dev/null || echo "unaliased"
