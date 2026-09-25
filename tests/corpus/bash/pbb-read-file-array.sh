# source: https://github.com/dylanaraps/pure-bash-bible#read-a-file-to-an-array-by-line
# desc: $(<file), IFS=$'\n' read -d "" -ra (drops empty lines), while-read array, mapfile
printf 'one\n\nthree\n' > file
file_data="$(<"file")"; printf '<%s>\n' "$file_data"
IFS=$'\n' read -d "" -ra file_data < "file"; printf '[%s]\n' "${file_data[@]}"
unset file_data
while read -r line; do
    file_data+=("$line")
done < "file"
printf '{%s}\n' "${file_data[@]}"
mapfile -t file_data < "file"; printf '(%s)\n' "${file_data[@]}"
