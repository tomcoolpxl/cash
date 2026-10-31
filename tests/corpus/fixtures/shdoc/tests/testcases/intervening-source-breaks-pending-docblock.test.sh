#!/usr/bin/env bash

tests:put input << EOF
#!/usr/bin/env bash

# @description Kept function.
kept() {
    :
}

# @description This block should not attach to later functions.
echo

actual() {
    :
}
EOF

tests:put expected << EOF
## Index

* [kept](#kept)

### kept

Kept function.
EOF

assert
