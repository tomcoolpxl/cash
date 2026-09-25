# source: https://github.com/dylanaraps/pure-bash-bible#split-a-string-on-a-delimiter
# desc: IFS=$'\n' read -d "" -ra arr <<< "${1//$2/$'\n'}"
split() {
   # Usage: split "string" "delimiter"
   IFS=$'\n' read -d "" -ra arr <<< "${1//$2/$'\n'}"
   printf '%s\n' "${arr[@]}"
}
split "apples,oranges,pears,grapes" ","
split "1, 2, 3, 4, 5" ", "
split "hello---world---my---name---is---john" "---"
