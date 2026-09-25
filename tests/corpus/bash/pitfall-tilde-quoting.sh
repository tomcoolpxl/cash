# source: https://mywiki.wooledge.org/BashPitfalls#echo_.22.7E.22
# desc: tilde expansion only when unquoted at word start
HOME='/home/my photos'
printf '%s\n' "~/dir with spaces" ~"/dir with spaces" ~/"dir with spaces" "$HOME/dir with spaces"
export foo=~/bar; echo "$foo"
x=a:~/b; echo "$x"
