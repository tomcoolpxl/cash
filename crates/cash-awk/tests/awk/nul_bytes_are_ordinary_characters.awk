# A NUL in a record, a field, a string or a regex is a character like any other.
{ print NF, length($0), length($1) }
$0 ~ /b/ { print "match", NR }
{ n = split($0, parts, "\0"); print n, parts[n] }
{ gsub(/b/, "B"); print }
END { s = "x\0y"; print length(s), index(s, "y") }
