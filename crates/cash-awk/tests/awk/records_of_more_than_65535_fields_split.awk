# A record of more than 65,535 fields, and a field assigned past them.
BEGIN {
    for (i = 1; i <= 70000; i++)
        s = s " w" i
    $0 = s
    print NF, $1, $65536, $NF
    $70002 = "z"
    print NF, $70002
}
