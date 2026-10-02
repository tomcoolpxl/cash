BEGIN {
    n = split("a.b.c", parts, "\.")
    print n, parts[1], parts[3]
    print "a\qb"
    s = "hello"
    sub(/l/, "[\&]", s)
    print s
    if ("a.c" ~ "a\.c") print "dynamic regex matches"
    print "tab[\t] quote[\"] slash[\/] octal[\101]"
    print "continued \
line"
}
