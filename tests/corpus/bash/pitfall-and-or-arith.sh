# source: https://mywiki.wooledge.org/BashPitfalls#cmd1_.26.26_cmd2_.7C.7C_cmd3
# desc: true && ((i++)) || ((i--)) runs both branches; brace group variant
i=0
true && ((i++)) || ((i--))  # WRONG!
echo "$i"                   # Prints 0
i=0
true && (( ++i )) || (( --i ))  # STILL WRONG!
echo "$i"                       # Prints 1 by dumb luck
true && { echo true; false; } || { echo false; true; }
