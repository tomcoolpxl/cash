# source: https://github.com/dylanaraps/pure-bash-bible#shorter-function-declaration
# desc: f()(echo), f()(($1)), f()if ..., f()for ...
f(){ echo hi;}
f
f()(echo hi sub)
f
f()(($1))
f 'a=1' ; echo "status=$? a=${a-unset}"
f 0 ; echo "status=$?"
f()if true; then echo "$1"; fi
f one
f()for i in "$@"; do echo "$i"; done
f x y z
