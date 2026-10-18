# source: https://github.com/dylanaraps/pure-bash-bible#obsolete-syntax
# desc: backquotes against $( ), nesting both ways, backslashes inside backquotes, and the function keyword
var="$(echo right)"
old=`echo wrong`
printf '%s %s\n' "$var" "$old"
nested="$(echo "outer $(echo inner)")"
printf '%s\n' "$nested"
bq=`echo outer \`echo inner\``
printf '%s\n' "$bq"
printf '%s\n' "`echo 'a\\b'`" "$(echo 'a\\b')"
printf '%s\n' "`echo \$HOME | wc -c | tr -d ' ' >/dev/null; echo dollar`"
do_something() {
    echo "plain function"
}
function do_other() {
    echo "function keyword with parens"
}
function do_third {
    echo "function keyword without parens"
}
do_something; do_other; do_third
declare -F | sort
