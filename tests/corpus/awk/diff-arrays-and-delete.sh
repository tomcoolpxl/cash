# source: tests/awk-differential.sh, case arrays-and-delete, frozen into the corpus
# desc: arrays-and-delete
printf '%b' '' | awk 'BEGIN {
    a["x"] = 10;
    a["y"] = 20;
    a["z"] = 30;
    delete a["y"];
    for (k in a) {
        # sort order might vary, but both x and z exist and y is deleted
        if (k == "x" || k == "z") print k, a[k];
    }
}'
