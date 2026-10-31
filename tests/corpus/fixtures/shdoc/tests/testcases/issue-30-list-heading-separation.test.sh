#!/usr/bin/env bash

tests:put input << EOF
# @description Sections after list output stay separate.
# @arg \$1 string Value to print.
# @exitcode 0 Success
# @stdout Printed value.
sections-after-lists() {
    :
}
EOF

tests:put expected << EOF
## Index

* [sections-after-lists](#sections-after-lists)

### sections-after-lists

Sections after list output stay separate.

#### Arguments

* **\$1** (string): Value to print.

#### Exit codes

* **0**: Success

#### Output on stdout

* Printed value.
EOF

assert
