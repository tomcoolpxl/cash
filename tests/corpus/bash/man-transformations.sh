# source: https://www.gnu.org/software/bash/manual/bash.html#Shell-Parameter-Expansion (${parameter@operator}; cash's own case)
# desc: @Q @E @P @A @a @U @u @L @K @k on scalars, indexed and associative arrays, and unset or empty values
s=$'it\'s a\ttab'
printf '[%s]\n' "${s@Q}" "${s@U}" "${s@u}" "${s@L}"
e='a\tb\x41\n'
printf '[%s]\n' "${e@E}"
declare -i n=42
declare -r ro=fixed
declare -x ex=exported
printf '[%s]\n' "${n@A}" "${ro@A}" "${ex@A}" "${n@a}" "${ro@a}" "${ex@a}" "${s@a}"
arr=(one 'two words' "it's")
printf '[%s]\n' "${arr[@]@Q}" "${arr[1]@Q}" "${arr@a}" "${arr[@]@U}"
printf '<%s>\n' "${arr[*]@Q}"
declare -A map=([k]='v w')
printf '[%s]\n' "${map[@]@Q}" "${map@a}" "${map[@]@K}" "${map[@]@k}"
idx=(x y)
printf '[%s]\n' "${idx[@]@K}"
empty=
printf '[%s]\n' "${empty@Q}" "${unset_v@Q}" "${unset_v@a}" "${empty@a}"
PS1_like='\u-not-expanded \\ \a'
printf '[%s]\n' "${PS1_like@P}" 2>/dev/null | sed 's/^\[[^-]*-/[USER-/'
set -- 'a b' c
printf '[%s]\n' "${@@Q}" "${1@U}"
