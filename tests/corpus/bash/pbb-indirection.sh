# source: https://github.com/dylanaraps/pure-bash-bible#assign-and-access-a-variable-using-a-variable
# desc: ${!ref}, declare -n ref=hello_$var, declare "hello_$var=value", ${!VAR*}
hello_world="value"
var="world"
ref="hello_$var"
printf '%s\n' "${!ref}"
declare -n nref=hello_$var
printf '%s\n' "$nref"
var2="there"
declare "hello_$var2=value2"
printf '%s\n' "$hello_there"
printf '%s\n' "${!hello_*}"
printf '<%s>\n' "${!hello_@}"
