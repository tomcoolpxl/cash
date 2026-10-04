# gawk's arrays of arrays: a subarray is an element, made by being used as one.

function walk(arr, prefix,    k) {
    for (k in arr) {
        if (isarray(arr[k]))
            walk(arr[k], prefix k "/")
        else
            print prefix k " = " arr[k]
    }
}

function fill(sub_array, n,    i) {
    for (i = 1; i <= n; i++)
        sub_array[i] = i * i
    return length(sub_array)
}

function count(sub_array) {
    return length(sub_array)
}

BEGIN {
    a[1][2] = "one-two"
    a[1][3] = "one-three"
    deep["x"]["y"]["z"] = "deep"
    a[5] = "scalar"
    walk(a, "")
    walk(deep, "")

    print "lengths", length(a), length(a[1]), length(deep["x"]["y"])
    print "isarray", isarray(a), isarray(a[1]), isarray(a[1][2]), isarray(a[5]), isarray(none)
    print "in", (2 in a[1]), (4 in a[1]), ("y" in deep["x"]), ((1, 2) in a)

    # An element only read has no type yet, and can still become a subarray.
    x = b[1]
    b[1][1] = "now an array"
    print "untyped", b[1][1], length(b)

    # A subarray passed to a function is the caller's.
    print "fill", fill(c["squares"], 4), c["squares"][4]
    print "count", count(c["squares"])
    fill(d[1], 2)
    print "made by the function", isarray(d[1]), length(d[1])

    n = split("p q r", e["words"])
    print "split", n, e["words"][1], e["words"][3]

    delete a[1][2]
    print "after delete", length(a[1]), (2 in a[1])
    delete a[1]
    print "after delete of a subarray", length(a), (1 in a)

    # Deleting from a subarray that is not there makes nothing.
    delete f[1][2]
    print "nothing made", length(f)

    g[1][2] = 3
    g[1][2] += 4
    g[1][2]++
    print "assigned", g[1][2]

    h["k"][1] = 1
    h["k"][2] = 2
    for (key in h["k"])
        sum += h["k"][key]
    print "loop", sum

    SUBSEP = ":"
    m[1, 2][3, 4] = "multi"
    for (i in m)
        for (j in m[i])
            print "subscripts", i, j, m[i][j]
}
