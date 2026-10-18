# source: https://github.com/dylanaraps/pure-bash-bible#default-value
# desc: :- - := = :+ + :? ? on unset, empty and set variables; the error forms end a subshell
unset u
e=
s=set
printf '[%s]\n' "${u:-dflt}" "${e:-dflt}" "${s:-dflt}"
printf '[%s]\n' "${u-dflt}" "${e-dflt}" "${s-dflt}"
printf '[%s]\n' "${u:+alt}" "${e:+alt}" "${s:+alt}"
printf '[%s]\n' "${u+alt}" "${e+alt}" "${s+alt}"
printf '[%s]\n' "${u:=assigned}" "$u"
unset u
printf '[%s]\n' "${e=kept-empty}" "$e" "${e:=now-set}" "$e"
( : "${unset_one:?is required}"; echo not reached )
echo "status $?"
( : "${e:?}"; echo "e is set, so this runs" )
echo "status $?"
empty=
( : "${empty?unset only}"; echo "empty is set" )
( : "${empty:?empty too}"; echo not reached )
echo "status $?"
arr=()
printf '[%s]\n' "${arr[0]:-no first}" "${arr[@]:-no elements}"
