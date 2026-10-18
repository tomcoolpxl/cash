# source: https://github.com/holman/spark/blob/master/spark (the fixture `spark`, unchanged)
# desc: a real script sourced: ${@//,/ } split into words, (( )) with << and 0xffffffff, arrays of multibyte ticks
. ./spark
spark 1 5 22 13 53
spark 0,30,55,80,33,150
spark 9 13 5 17 1
spark 5 5 5
spark 1.9 2.1 3.7
spark -3 0 3
spark 100
spark 0 1000000
_echo -n "no newline"; _echo " then one"
_echo -n
_echo
