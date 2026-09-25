# source: https://mywiki.wooledge.org/BashPitfalls#cmd_.3E_.22file.24.28.28i.2B.2B.29.29.22
# desc: side effects in redirection words ({ cmd ;} > "file$((i++))" and temp var)
shopt -s nullglob
i=0
file=file$(( i++ ))
echo arg > "$file"
{ echo arg ;} > "file$(( i++ ))"
declare -p i
files=( file* ); declare -p files
