# More dynamic patterns than the compiled-pattern cache holds, each used again after the
# cache was emptied: every match is still decided by its own pattern.
BEGIN {
    for (round = 1; round <= 2; round++)
        for (i = 1; i <= 150; i++) {
            pattern = "^a" i "b$"
            if (("a" i "b") ~ pattern) hits++
            if (("a" i "bc") ~ pattern) misses++
        }
    print hits + 0, misses + 0
}
