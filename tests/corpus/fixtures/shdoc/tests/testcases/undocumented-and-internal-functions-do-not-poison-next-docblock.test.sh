#!/usr/bin/env bash

tests:put input << EOF
#!/usr/bin/env bash

undocumented() {
    :
}

# @internal
internal_helper() {
    :
}

# @internal
echo "not a function"

# @description Visible function.
visible() {
    :
}
EOF

tests:put expected << EOF
## Index

* [visible](#visible)

### visible

Visible function.
EOF

assert
