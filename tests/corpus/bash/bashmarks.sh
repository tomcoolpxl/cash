# source: https://github.com/huyng/bashmarks/blob/master/bashmarks.sh (the fixture `bashmarks.sh`, unchanged)
# desc: a real script sourced: bookmarks kept as exported DIR_ variables in a file it sources again, names checked with sed, eval of an echo of \$DIR_name, env | sort | awk with split and printf, and cd to a bookmark
export SDIRS="$PWD/sdirs"
. ./bashmarks.sh
mkdir -p proj/src docs
cd proj/src && s src && cd ../../docs && s docs && cd ..
l | sed 's|  *.*/| |'
p src | sed 's|.*/||'
g src; basename "$PWD"
g docs; basename "$PWD"
s bad-name
d src; l | sed 's|  *.*/| |'
_l
g nosuch
