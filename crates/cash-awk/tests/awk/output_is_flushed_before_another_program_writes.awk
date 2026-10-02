BEGIN {
    printf "a"
    system("echo b")
    print "c"
    print "d" > "/dev/stdout"
    print "e"
}
