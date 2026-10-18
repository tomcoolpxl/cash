# source: https://github.com/dylanaraps/pure-bash-bible#arithmetic-operators
# desc: every arithmetic, bitwise, logical and comma operator, the assignment forms, and their statuses
a=17 b=5
echo $((a+b)) $((a-b)) $((a*b)) $((a/b)) $((a**2)) $((a%b)) $((-a/b)) $((-a%b))
x=10; ((x+=3)); echo $x; ((x-=4)); echo $x; ((x*=3)); echo $x; ((x/=4)); echo $x; ((x%=4)); echo $x
echo $((1<<4)) $((256>>3)) $((12&10)) $((12|10)) $((~12)) $((12^10))
y=3; ((y<<=2)); echo $y; ((y>>=1)); echo $y; ((y&=5)); echo $y; ((y|=8)); echo $y; ((y^=15)); echo $y
echo $((!0)) $((!7)) $((1&&0)) $((1||0)) $((0||0)) $((2&&3))
((a=1,b=2,c=3)); echo "$a $b $c" $((a=4,b+a))
((0)); echo "status $?"
((5)); echo "status $?"
((z=0)); echo "status $? z=$z"
echo $((2**10)) $((2**62)) $((2**63)) $((-2**2)) $(( (-2)**2 ))
echo $((0x1F)) $((017)) $((2#1011)) $((36#z)) $((64#_))
echo $((7>3?10:20)) $((1==1)) $((2!=2)) $((3<=3)) $((4>=5))
i=5; echo $((i++)) $i $((++i)) $((i--)) $((--i))
