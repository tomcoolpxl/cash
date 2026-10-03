# An RS that can match empty text ends a record only where it matches some text.
BEGIN { RS = "X*" }
{ print NR, "[" $0 "]" }
