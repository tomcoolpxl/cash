#!/usr/bin/env bash

tests:put input << EOF
if true; then
    # @description First line.
    # Second line from an indented comment.
    #
    # @noargs
    nested() {
        :
    }
fi
EOF

tests:put expected << EOF
## Index

* [nested](#nested)

### nested

First line.
Second line from an indented comment.

_Function has no arguments._
EOF

assert
