# source: tests/awk-differential.sh, case string-sub-gsub, frozen into the corpus
# desc: string-sub-gsub
printf '%b' 'a-b-c-d\n' | awk '{
    s = $0;
    sub(/-/, ":", s);
    print s;
    gsub(/-/, "_", $0);
    print $0;
}'
