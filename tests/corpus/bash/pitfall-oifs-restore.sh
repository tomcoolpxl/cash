# source: https://mywiki.wooledge.org/BashPitfalls#OIFS.3D.22.24IFS.22.3B_.2026.3B_IFS.3D.22.24OIFS.22
# desc: preserve set/unset state of IFS with ${IFS+_${IFS}} and ${oIFS:+'false'}
array=(a b c)
unset IFS
oIFS=${IFS+_${IFS}}
IFS=/; printf %s\\n "${array[*]}"
${oIFS:+'false'} unset -v IFS || IFS=${oIFS#_}
echo "IFS set? ${IFS+yes}"
f() {
  local IFS
  IFS=/; printf %s\\n "${array[*]}"
}
f
( IFS=/; printf %s\\n "${array[*]}" )
