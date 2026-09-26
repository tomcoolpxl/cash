function first_key(arr,    k) {
    for (k in arr)
        return k
}

BEGIN {
    a["only"] = 1

    # break leaves the for-in loop, not the enclosing while
    n = 0
    while (n < 3) {
        n++
        for (k in a) {
            if (1) {
                print "for-in body " n
                break
            }
            print "not reached"
        }
        print "after for-in " n
    }

    # continue goes on to the next key
    b[1]; b[2]; b[3]
    count = 0
    for (k in b) {
        if (k != 2)
            continue
        count++
    }
    print "continue skipped to " count

    # a break must pop the iterator: 100000 of them would overflow the stack
    for (i = 0; i < 100000; i++)
        for (k in a)
            break
    print "many breaks ok " first_key(a)

    # do-while: break ends it, continue re-tests the condition
    i = 0
    do {
        i++
        if (i == 2)
            continue
        if (i == 4)
            break
        print "do-while " i
    } while (i < 10)
    print "do-while stopped at " i
}
