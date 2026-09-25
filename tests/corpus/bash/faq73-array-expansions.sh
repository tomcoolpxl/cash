# source: https://mywiki.wooledge.org/BashFAQ/073 (Parameter Expansion on Arrays)
# desc: per-element # % // /# /% on arrays; prefixing; ${@: -2:1}
a=(alpha beta gamma)
echo "${a[@]#a}"
echo "${a[@]%a}"
echo "${a[@]//a/f}"
echo "${a[@]/#a/f}"
echo "${a[@]/%a/f}"
echo "${a[@]/#/a}"
echo "${a[@]/%/a}"
PFX=inc_
a=("${a[@]/#/$PFX}")
echo "${a[@]}"
set -- one two three four
echo "${@:(-2):1}" "${@: -2:1}"
