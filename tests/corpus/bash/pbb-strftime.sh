# source: https://github.com/dylanaraps/pure-bash-bible#get-the-current-date-using-strftime
# desc: printf %(fmt)T on fixed times in TZ=UTC, printf -v, and a function named date shadowing the program
export TZ=UTC
date() {
    # Usage: date "format"
    printf "%($1)T\\n" "${2:--1}"
}
date "%a %d %b  - %l:%M %p" 0
date "%Y-%m-%d %H:%M:%S" 86399
date "%j %U %u %w %y %C %e" 1700000000
printf '%(%a %d %b  - %l:%M %p)T\n' 951782400
printf -v when '%(%F %T)T' 1234567890
printf '[%s]\n' "$when"
printf '%(%s)T|%(%%)T|%()T\n' 42 42 0
[[ $(date "%Y" -1) == 20[0-9][0-9] ]] && echo "-1 is now"
