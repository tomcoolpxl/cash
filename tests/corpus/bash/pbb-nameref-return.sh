# source: https://github.com/dylanaraps/pure-bash-bible#capture-the-return-value-of-a-function-without-command-substitution
# desc: local -n nameref to return a value
to_upper() {
  local -n ptr=${1}

  ptr=${ptr^^}
}

foo="bar"
to_upper foo
printf "%s\n" "${foo}" # BAR
