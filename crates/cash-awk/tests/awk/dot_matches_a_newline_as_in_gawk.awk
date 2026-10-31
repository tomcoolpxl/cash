# In awk a string may hold newlines, and `.` matches one, as gawk has it: shdoc's
# `gensub("^.*$", "  &", "g", text "\n")` indents a definition that ends in a newline
# (2026-10-31). `$` still means the end of the string alone, and a negated bracket
# expression matches a newline too.
BEGIN {
    s = "a\nb"
    print (s ~ /^a.b$/) ? "dot matches newline" : "dot: no"
    t = "x\n"
    print "[" gensub("^.*$", "  &", "g", t) "]"
    u = "p\nq"
    sub(/p.q/, "R", u)
    print "[" u "]"
    print ("a\n" ~ /a$/) ? "dollar: yes" : "dollar: no"
    v = "line1\nline2"
    n = gsub(/[^\n]+/, "<&>", v)
    print n ": " v
    w = "one\ntwo"
    print match(w, /e.t/) ": " RSTART " " RLENGTH
}
