function drop_x() {
    delete a["x"]
    return 7
}

BEGIN {
    b["x"] = split("p q r", b)
    print length(b), b["x"], b[1], b[3]

    a["x"] = 1; a["z"] = 2
    a["y"] = drop_x()
    print length(a), a["y"], ("x" in a), a["z"]
}
