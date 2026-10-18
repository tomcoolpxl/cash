# source: https://github.com/dylanaraps/pure-bash-bible#name-a-variable-based-on-another-variable
# desc: ${!ref}, declare -n ref=hello_$var, declare "hello_$var=value", ${!prefix*} and ${!prefix@}
hello_world="value"
var="world"
ref="hello_$var"
printf '%s\n' "${!ref}"
declare -n nref=hello_$var
printf '%s\n' "$nref"
nref="changed through the nameref"
printf '%s\n' "$hello_world"
unset -n nref
var="planet"
declare "hello_$var=value two"
printf '%s\n' "$hello_planet"
hello_zzz=3
printf '<%s>\n' ${!hello_*}
printf '[%s]\n' "${!hello_@}"
IFS=,
printf '{%s}\n' "${!hello_*}"
unset IFS
printf '(%s)\n' ${!nosuchprefix_*}
