function has(arr, x,    k) {
    for (k in arr)
        if (arr[k] == x)
            return 1
    return 0
}

BEGIN {
    split("p q p r q s", words, " ")
    for (i = 1; i <= 6; i++)
        if (!has(seen, words[i]))
            seen[n++] = words[i]
    print "distinct", n

    for (k in seen) break
    seen["after-break"] = 1
    print "after break", length(seen)

    count = 0
    for (k in seen) { added[k] = 1; seen[k "+"] = 1; count++ }
    print "visited", count, "now", length(seen)

    for (k in seen) delete seen
    print "deleted", length(seen)

    a["x"]; for (k in a) if (a["missing"]) print "never"
    print "read a missing key in a loop", length(a)
}
