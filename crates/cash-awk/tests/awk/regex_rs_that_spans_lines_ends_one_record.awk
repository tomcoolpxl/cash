# RS = "\n+" ends a record at a run of newlines, however long, not at each one.
BEGIN { RS = "\n+" }
{ print NR ": " $0 }
