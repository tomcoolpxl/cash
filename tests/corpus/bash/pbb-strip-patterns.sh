# source: https://github.com/dylanaraps/pure-bash-bible#strip-all-instances-of-pattern-from-string
# desc: ${1//$2}, ${1/$2}, ${1##$2}, ${1%%$2} with bracket and class patterns
strip_all() { printf '%s\n' "${1//$2}"; }
strip() { printf '%s\n' "${1/$2}"; }
lstrip() { printf '%s\n' "${1##$2}"; }
rstrip() { printf '%s\n' "${1%%$2}"; }
strip_all "The Quick Brown Fox" "[aeiou]"
strip_all "The Quick Brown Fox" "[[:space:]]"
strip_all "The Quick Brown Fox" "Quick "
strip "The Quick Brown Fox" "[aeiou]"
strip "The Quick Brown Fox" "[[:space:]]"
lstrip "The Quick Brown Fox" "The "
rstrip "The Quick Brown Fox" " Fox"
