# source: https://mywiki.wooledge.org/BashPitfalls#local_var.3D.24.28cmd.29
# desc: local var=$(cmd) masks the exit status; separate declaration keeps it
f() { local var=$(false); echo "local: $?"; local v2; v2=$(false); echo "separate: $?"; }
f
g() { export e=$(exit 3); echo "export: $?"; readonly r=$(exit 4); echo "readonly: $?"; }
g
