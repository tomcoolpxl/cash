# source: https://github.com/dylanaraps/pure-bash-bible#simpler-syntax-to-set-variables
# desc: ((var=1+2)), ++ -- += -=, variables and array elements inside (( )), and the ternary
((var=1+2)); echo "$var"
((var++)); echo "$var"
((var--)); echo "$var"
((var+=1)); echo "$var"
((var-=1)); echo "$var"
var2=4
arr=(1 2 7)
((var=var2*arr[2])); echo "$var"
idx=1
((arr[idx]+=40)); echo "${arr[@]}"
((var=var2>var?var2:var)); echo "$var"
var2=100
((var=var2>var?var2:var)); echo "$var"
declare -i n=3
n+=4; echo "$n"
n="n*2"; echo "$n"
unset count
((count++)); echo "count=$count status=$?"
