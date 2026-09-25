# source: https://mywiki.wooledge.org/BashFAQ/073
# desc: # ## % %% on a dotted hostname and a path; nested quote strip
name=polish.ostrich.racing.champion
echo "${name#*.}"; echo "${name##*.}"; echo "${name%%.*}"; echo "${name%.*}"
file=/usr/share/java-1.4.2-sun/demo/applets/Clock/Clock.class
echo "${file#*/}"; echo "${file##*/}"; echo "[${file%%/*}]"; echo "${file%/*}"
foo='key="some value"'
bar=${foo#*=\"} bar=${bar%\"*}
echo "$bar"
string=abcdef
echo "${string:2:1} ${string:1} ${string%?} ${string: -1} ${string:(-1)} ${string:-1}"
