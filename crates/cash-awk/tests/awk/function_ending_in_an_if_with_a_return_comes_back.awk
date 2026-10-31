# A function whose last statement is an `if` holding a `return`, where the condition is
# false, falls off its end: the caller's next statements still run (shdoc's
# handle_description, 2026-10-31), in a rule's action, in BEGIN and in another function.
function f() {
    if (x == "never") {
        x = 1
        return
    }
}

function g() {
    f()
    print "in g after f"
}

function h(v) {
    if (v > 1) return v
}

BEGIN {
    f()
    print "begin after f"
    g()
    print "begin after g"
    print "h: [" h(1) "] [" h(2) "]"
}

/^$/ {
    f()
    print "action after f"
    next
}

{
    print "NOT HANDLED: " $0
}
