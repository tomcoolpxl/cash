# source: https://mywiki.wooledge.org/BashPitfalls#IFS.3D.2C_read_-ra_fields_.3C.3C.3C_.22.24csv_line.22
# desc: IFS as field terminator: trailing empty field dropped by read -ra; rest-of-line stripping
IFS=, read -ra fields <<< "a,b,"
declare -p fields
IFS=, read -ra fields <<< "a,b,c"
declare -p fields
input="a,b,"
IFS=, read -ra fields <<< "$input,"
declare -p fields
echo 'foo:bar:' | { IFS=: read -r f1 rest; declare -p f1 rest; }
echo 'foo:bar:f' | { IFS=: read -r f1 rest; declare -p rest; }
echo 'foo:bar::' | { IFS=: read -r f1 rest; declare -p rest; }
input='foo:bar:'
echo "$input:" | { IFS=: read -r f1 rest; rest=${rest%:}; declare -p rest; }
