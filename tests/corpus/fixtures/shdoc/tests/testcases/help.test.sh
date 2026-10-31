#!/usr/bin/env bash

tests:ensure ./shdoc --help
tests:assert-stdout-re '^Usage: shdoc \[OPTIONS\] \[FILE\]'
tests:assert-stdout 'Generate Markdown documentation from shell source comments.'
tests:assert-stdout '  -h, --help       Show this help message and exit.'
tests:assert-stdout '      --version    Show version information and exit.'
tests:assert-stderr-empty

tests:ensure ./shdoc -h
tests:assert-stdout-re '^Usage: shdoc \[OPTIONS\] \[FILE\]'
tests:assert-stderr-empty

tests:ensure ./shdoc --version
tests:assert-stdout-re '^shdoc v'
tests:assert-stderr-empty
