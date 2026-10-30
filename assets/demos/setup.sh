#!/bin/bash
# Makes the directory the demo clips run in: $PUBLIC/cash-demo, a small project with
# sources, logs, data and a script, plus a history for the prompt clip. Idempotent: it
# starts from nothing every time, so every render shows the same thing. Run by
# scripts\render-demos.ps1; by hand: `cash assets/demos/setup.sh`.
set -euo pipefail

demo="$PUBLIC/cash-demo"
rm -rf "$demo"
mkdir -p "$demo/project/src" "$demo/project/logs" "$demo/project/data" \
    "$demo/project/scripts" "$demo/project/build" "$demo/notes" "$demo/photos"
cd "$demo/project"

cat > README.md <<'EOF'
# inventory

A small service that counts things. `scripts/report.sh` prints the daily report.
EOF

cat > src/main.rs <<'EOF'
mod lib;
mod util;

fn main() {
    // TODO: read the port from the environment
    let port = 8080;
    println!("listening on {port}");
    util::serve(port);
}
EOF

cat > src/lib.rs <<'EOF'
pub struct Item {
    pub name: String,
    pub count: u32,
}

// TODO: persist the items
pub fn items() -> Vec<Item> {
    Vec::new()
}
EOF

cat > src/util.rs <<'EOF'
pub fn serve(port: u16) {
    // TODO: real server
    // TODO: graceful shutdown
    let _ = port;
}
EOF

# A log with Windows line endings, as a Windows service writes one.
printf '%s\r\n' \
    '[2026-10-30 08:00:01] INFO  service started on :8080' \
    '[2026-10-30 08:00:02] INFO  loaded 1204 items' \
    '[2026-10-30 08:14:20] ERROR item 77: count below zero' \
    '[2026-10-30 09:02:11] INFO  report sent' \
    '[2026-10-30 09:30:45] ERROR database locked, retrying' \
    '[2026-10-30 09:30:47] INFO  retry succeeded' \
    '[2026-10-30 11:12:09] ERROR item 102: name is empty' \
    '[2026-10-30 11:40:00] INFO  report sent' \
    > logs/app.log
printf '%s\r\n' '[2026-10-29 23:59:59] INFO  rotated' > logs/old.log

cat > data/users.csv <<'EOF'
id,name,city
1,Ada,Ghent
2,Bram,Antwerp
3,Chloe,Ghent
4,Dries,Hasselt
5,Els,Ghent
6,Finn,Antwerp
7,Greet,Leuven
EOF

cat > scripts/report.sh <<'EOF'
#!/bin/bash
# The daily report: find, xargs, grep, sed and awk, as on Linux.
set -euo pipefail

echo "TODOs per file:"
find src -name '*.rs' -print0 | xargs -0 grep -c TODO | sed 's/:/  /'

echo "Errors today:"
grep ERROR logs/app.log | sed -E 's/^\[([^]]+)\] ERROR /\1  /'

echo "Users per city:"
awk -F, 'NR > 1 { n[$3]++ } END { for (c in n) print n[c], c }' data/users.csv | sort -rn
EOF
chmod +x scripts/report.sh

# The program that holds logs/app.log open in the ports clip, as a service would.
printf '%s\r\n' \
    '$held = [IO.File]::Open("logs/app.log", "Open", "Read", "None")' \
    'Start-Sleep 100' \
    > scripts/hold.ps1

printf 'x' > build/inventory.exe
printf 'notes\n' > "$demo/notes/todo.txt"

# The history the prompt clip's suggestions and Ctrl-R come from, oldest first.
printf '%s\n' \
    'ls -l logs' \
    'grep -c ERROR logs/app.log' \
    "awk -F, 'NR > 1 { print \$3 }' data/users.csv | sort | uniq -c" \
    'cargo test --workspace' \
    'git status' \
    > "$demo/.history"
