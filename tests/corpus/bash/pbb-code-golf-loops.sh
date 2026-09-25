# source: https://github.com/dylanaraps/pure-bash-bible#shorter-for-loop-syntax
# desc: for((;i++<10;)){ ...;} and undocumented for i in {1..10};{ ...;}
for((;i++<10;)){ echo "$i";}
for i in {1..3};{ echo "$i";}
