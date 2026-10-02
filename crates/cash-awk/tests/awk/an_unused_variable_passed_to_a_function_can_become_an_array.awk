function fill(a) { a[1] = 5; a["k"] = "v" }
function set_scalar(s) { s = 9; return s }
function count(a,    k, n) { for (k in a) n++; return n }
function nested(a) { fill(a) }
function split_into(a) { return split("x y z", a) }
function two(a, b) { a = 1; b[1] = 2 }

BEGIN {
    fill(arr)
    print arr[1], arr["k"], count(arr)

    print set_scalar(sc), "[" sc "]"

    nested(deep)
    print deep[1]

    print split_into(parts), parts[3]

    two(same, other)
    print "[" same "]", other[1]
}
