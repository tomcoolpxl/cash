# source: https://github.com/dylanaraps/pure-bash-bible#replacement
# desc: every # ## % %% / // form, with patterns, an anchored /#, /% and & in the replacement
p=/home/user/archive.tar.gz
printf '%s\n' "${p#*/}" "${p##*/}" "${p%.*}" "${p%%.*}"
printf '%s\n' "${p/a/A}" "${p//a/A}" "${p/a}" "${p//a}"
printf '%s\n' "${p/#\/home//srv}" "${p/%gz/bz2}" "${p//[aeiou]/_}"
shopt -s extglob
printf '%s\n' "${p//+([a-z])/x}" "${p##+([!.])}"
shopt -s patsub_replacement
printf '%s\n' "${p//a/<&>}" "${p//a/\&}"
shopt -u patsub_replacement
printf '%s\n' "${p//a/<&>}"
arr=(one.txt two.txt three.md)
printf '%s\n' "${arr[@]%.txt}" "${arr[@]/#/pre-}" "${arr[*]//o/0}"
empty=
printf '[%s]\n' "${empty#x}" "${unset_var//a/b}"
