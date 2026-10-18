# source: https://www.gnu.org/software/bash/manual/bash.html#index-case (cash's own case)
# desc: case with ;; ;& and ;;&, patterns with | and classes, nocasematch, and the status of a case
for w in apple Banana cherry 42 ''; do
    case $w in
        a*) echo "$w: starts with a" ;&
        *e*) echo "$w: has an e (or fell through)" ;;&
        [[:upper:]]*) echo "$w: capital" ;;
        *[0-9]*|'') echo "$w: digits or empty" ;;
        *) echo "$w: other" ;;
    esac
done
case x in y) echo no ;; esac; echo "no match status $?"
case x in x) false ;; esac; echo "matched status $?"
shopt -s nocasematch
case BANANA in banana) echo "nocasematch case" ;; esac
[[ HELLO == hel* ]] && echo "nocasematch [[ ]]"
shopt -u nocasematch
case 'a|b' in 'a|b') echo "quoted bar is literal" ;; esac
pat='*.txt'
case notes.txt in $pat) echo "unquoted variable is a pattern" ;; esac
case notes.txt in "$pat") echo no ;; *) echo "quoted variable is literal" ;; esac
case ']' in []]) echo "bracket in a class" ;; esac
case '-' in [a-]) echo "dash at the end of a class" ;; esac
case '!' in [!!]) echo no ;; *) echo "negated bang class" ;; esac
