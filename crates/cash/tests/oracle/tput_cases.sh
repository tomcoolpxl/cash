# tput cases, run under ncurses' tput (the oracle) and under cash's builtin, for the
# terminal xterm-256color. Output is shown through od so escape sequences are visible.
# tput_cases.out is ncurses 6.6.20251230's output.
#
# Regenerate the golden file (WSL):
#   TERM=xterm-256color bash tput_cases.sh > tput_cases.out 2>&1
#
# cols and lines depend on the window, so only their shape is checked.

exec 2>&1
export TERM=xterm-256color
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
t() { printf '%s: ' "$*"; tput "$@" | show; echo "rc=${PIPESTATUS[0]}"; }
s() { printf -- '-S [%s]: ' "$1"; printf '%s\n' "$1" | tput -S | show; echo "rc=${PIPESTATUS[1]}"; }

echo "== every string capability xterm-256color has, without parameters"
for c in acsc bel blink bold cbt civis clear cnorm cr csr cub cub1 cud cud1 cuf cuf1 cup \
    cuu cuu1 cvvis dch dch1 dim dl dl1 ech ed el el1 flash home hpa ht hts ich il il1 ind \
    indn initc invis is2 kDC kEND kHOM kIC kLFT kNXT kPRV kRIT ka1 ka3 kb2 kbeg kbs kc1 \
    kc3 kcbt kcub1 kcud1 kcuf1 kcuu1 kdch1 kend kent kf1 kf2 kf3 kf4 kf5 kf6 kf7 kf8 kf9 \
    kf10 kf11 kf12 khome kich1 kind kmous knp kpp kri mc0 mc4 mc5 meml memu mgc nel oc op \
    rc rep rev ri rin ritm rmacs rmam rmcup rmir rmkx rmm rmso rmul rs1 rs2 sc setab \
    setaf sgr sgr0 sitm smacs smam smcup smglp smglr smgrp smir smkx smm smso smul tbc \
    u6 u7 u8 u9 vpa \
    BD BE Cr Cs E3 Ms PE PS RV Se Ss XM XR fd fe ka2 kb1 kb3 kc2 kp5 kpADD kpCMA kpDIV \
    kpDOT kpMUL kpSUB kpZRO kxIN kxOUT rmxx rv smxx xm xr; do
    printf '%s=' "$c"; tput "$c" | show
done
echo "== function keys with modifiers"
for n in $(seq 13 63); do printf 'kf%s=' "$n"; tput "kf$n" | show; done
echo "== cursor and editing keys with modifiers"
for k in kDC kDN kEND kHOM kIC kLFT kNXT kPRV kRIT kUP; do
    for m in 3 4 5 6 7; do printf '%s%s=' "$k" "$m"; tput "$k$m" | show; done
done
t kDN
t kUP

echo "== colours"
for n in 0 7 8 15 16 200 255 256 2147483647; do t setaf "$n"; done
for n in 0 7 8 15 16 42 255; do t setab "$n"; done
t setaf
t setaf abc
t setaf 7 8
t setaf -0

echo "== cursor movement"
t cup 5 10
t cup 5
t cup 0 0
t cup 0x10 010
t cup +5 3
t cup ' 5' 3
t cup 5.5 3
t cup 08 3
t cup 99999999999 3
t cup 2147483647 1
t cup 4294967295 1
t cup 1 2 3 4 5 6 7 8 9 10 11
t cup x y
t cup
t hpa 4
t vpa 4
t hpa
t cuu
t cuu 0
t cuu 3
t cuu 10
t cud 2
t cuf 2
t cub 2
t csr 2 20
t csr 5
t csr 0 0
t indn 3
t rin 3
t il 2
t dl 2
t ich 2
t dch 2
t ech 3
t smglp 3
t smgrp 3
t smglr 1 10
t bold 1
t el 1
t kbs 1
t E3 1

echo "== sgr, rep, initc, the termcap-style u6, the texts of Cs and Ms"
t sgr 0 0 0 0 0 0 0 0 0
t sgr 1 1 1 1 1 1 1 1 1
t sgr 0 1
t sgr 1 1
t sgr 0 0 1
t sgr 0 0 0 1
t sgr 0 0 0 0 1
t sgr 0 0 0 0 0 1
t sgr 0 0 0 0 0 0 1
t sgr 0 0 0 0 0 0 0 1
t sgr 0 0 0 0 0 0 0 0 1
t sgr 2 0 3
t sgr 1 0 0 0 0 0 0 0 1
t sgr 0 0 0 0 0 0 0 0 0 1
t rep 65 3
t rep 65 1
t rep 65 0
t rep 65
t rep 300 2
t rep 0 2
t rep a 3
t rep 65 2147483647
t rep 65 2147483648
t initc 1 1000 500 0
t initc 0 0 0 0
t initc 7 1000 1000 1000
t initc 1 0x10 1 1
t initc 1 1001 2000 999
t u6 1 2
t u6 1
t u6 1 2 3
t u8 1
t xm 1 2 3 4
t Cs red
t Cs red bold
t Ms a b
t Ss 2

echo "== several capabilities on one command line"
t cup 5 10 bold
t bold setaf 2
t bold 1 setaf 2
t setaf 2 x
t cup 1 2 3 4 5 6 7 8 9 10 11 bold
t sgr 1 bold
t am bold
t xon bold
t setf 1 bold
t kf0 bold
t longname bold
t clear bold
t init bold
t bold nosuch sgr0
t nosuch bold
t bold ''
t bold -- -1
t -- bold
t ''
t 0
t Clear
t md

echo "== commands"
t longname
t clear
t -x clear
t -xT xterm clear
t -vx clear
t clear 1
t init
t -x init
t reset
t reset 1

echo "== booleans"
for b in am bce ccc km mc5i mir msgr npc xenl AX XF XT OTbs xon hs eo gn bw da chts; do
    tput "$b"; echo "$b rc=$?"
done
t am 1
t xon 1

echo "== numbers"
for n in colors pairs it xmc ncv lm wsl btns; do t "$n"; done
t U8
t it 1
echo "cols and lines are positive numbers:"
cols=$(tput cols); lines=$(tput lines)
[ "$cols" -gt 0 ] && echo "cols positive"
[ "$lines" -gt 0 ] && echo "lines positive"
printf 'cols lines\n' | tput -S | wc -l | tr -d ' '
COLUMNS=133 tput cols; LINES=9 tput lines
echo "exported COLUMNS wins:"; export COLUMNS=131; tput cols; unset COLUMNS

echo "== strings xterm-256color lacks"
for c in setf setb smgl kf0 rs3 cvvis; do tput "$c" | show; echo "$c rc=${PIPESTATUS[0]}"; done

echo "== -S"
s 'bold'
s 'cup 5 10 20'
s 'cup 5 10 bold'
s 'setaf x'
s 'bold x'
s 'bold -1'
s 'bold -x'
s 'bold - 1'
s 'bold 5 -1'
s 'bold +5'
s 'bold 0x1f'
s 'bold 077'
s 'bold 08'
s 'bold 1e3'
s 'cup -1 3'
s 'cup +5 +3'
s 'clear'
s 'init'
s 'reset'
s 'longname'
s 'E3'
s 'colors'
s 'am'
s 'xon'
s 'xon bold'
s 'xon kf0'
s 'xon xon xon'
s 'bold xon'
s 'setf 1 bold'
s 'kf0 bold'
s 'a b c'
s 'am 1'
s 'it 1'
printf -- '-S several lines: '; printf 'bold\nsetaf 2\nnosuch\nsgr0\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S blank lines: '; printf '\n\nbold\n\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S no final newline: '; printf 'bold\nsetaf 3' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S spaces and tabs: '; printf '  bold   \n\tsetaf\t2\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S CRLF: '; printf 'bold\r\nsetaf 2\r\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S after an unknown name: '; printf 'nosuch\nbold\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S numbers and a string: '; printf 'colors\nbold\n' | tput -S | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S two failing lines: '; printf 'xon kf0\nxon kf0\n' | tput -S; echo "rc=$?"
printf -- '-S empty input: '; printf '' | tput -S; echo "rc=$?"
printf -- '-S with an argument: '; printf 'bold\n' | tput -S bold | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S -x clear: '; printf 'clear\n' | tput -S -x | show; echo "rc=${PIPESTATUS[1]}"
printf -- '-S -S: '; printf 'bold\n' | tput -S -S | show; echo "rc=${PIPESTATUS[1]}"

echo "== options"
t -T xterm bold
t -Txterm bold
t -T vt100 bold
t -T ms-terminal bold
tput -T foo bel; echo "rc=$?"
printf 'bold\n' | tput -S -T foo; echo "rc=$?"
tput -T '' bold; echo "rc=$?"
tput -T=xterm bold; echo "rc=$?"
tput -T foo -V >/dev/null; echo "-T foo -V rc=$?"
tput -V >/dev/null; echo "-V rc=$?"
tput -V bold >/dev/null; echo "-V bold rc=$?"
tput; echo "rc=$?"
tput -T; echo "rc=$?"
tput -Z bold; echo "rc=$?"
tput -Sbold; echo "rc=$?"
tput cuu -3; echo "rc=$?"
tput -- -x; echo "rc=$?"
tput -v nosuch; echo "rc=$?"
echo "== the version"
tput -V
echo "== TERM unset or not VT: ignored by cash, whose terminal is VT"
env -u TERM tput bold | show; echo "rc=${PIPESTATUS[0]}"
TERM=dumb tput bold | show; echo "rc=${PIPESTATUS[0]}"
