#!/usr/bin/env bash

tests:put input << EOF
# @description Asserts that a variable is an array.
# @arg \$1 string The name of the variable to check.
# @exitcode 0 If the assertion succeeded.
# @exitcode 1 If the assertion failed.
# @exitcode 127 If the wrong number of arguments were provided.
# @stderr The error message if the assertion fails.
stdlib_array_assert_is_array() { :; }
EOF

tests:put expected << EOF
## Index

* [stdlib_array_assert_is_array](#stdlib_array_assert_is_array)

### stdlib_array_assert_is_array

Asserts that a variable is an array.

#### Arguments

* **\$1** (string): The name of the variable to check.

#### Exit codes

* **0**: If the assertion succeeded.
* **1**: If the assertion failed.
* **127**: If the wrong number of arguments were provided.

#### Output on stderr

* The error message if the assertion fails.

EOF

assert
