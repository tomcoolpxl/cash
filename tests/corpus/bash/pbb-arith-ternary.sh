# source: https://github.com/dylanaraps/pure-bash-bible#ternary-tests
# desc: ((var=1+2)), ++/--/+=, array element in arithmetic, ternary max
((var=1+2)); echo $var
((var++)); ((var--)); ((var+=1)); ((var-=1)); echo $var
arr=(0 0 7); var2=3
((var=var2*arr[2])); echo $var
var=5; var2=9
((var=var2>var?var2:var)); echo $var
((a=1,b=2,c=3)); echo $a$b$c
