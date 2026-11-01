BEGIN {
    # Assigning a high field index grows the record; intervening fields are
    # created as null strings, as gawk has them, and NF is updated. No silent cap.
    $2000 = "z"
    print NF
    print $2000
    print ($1500 == 0)
}
