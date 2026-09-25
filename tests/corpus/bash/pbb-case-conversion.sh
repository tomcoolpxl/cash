# source: https://github.com/dylanaraps/pure-bash-bible#change-a-string-to-lowercase
# desc: ${1,,} ${1^^} ${1~~}
lower() { printf '%s\n' "${1,,}"; }
upper() { printf '%s\n' "${1^^}"; }
reverse_case() { printf '%s\n' "${1~~}"; }
lower "HeLlO"; upper "HeLlO"; reverse_case "HeLlO"; reverse_case "hello"
