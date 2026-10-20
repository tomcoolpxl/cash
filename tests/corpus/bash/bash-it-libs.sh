# source: https://github.com/Bash-it/bash-it/tree/ee8396b (`lib/log.bash` and `lib/utilities.bash`, unchanged, with its LICENSE, as the fixture directory `bash-it`)
# desc: bash-it's libraries loaded under set -a, as its bats tests load them, and a child shell that finds them exported: log levels set with ${x:=n} and made read-only through ${!prefix@}, messages by level, a log prefix taken from a component's path, a component's name and type, an array search, dedup through sort, singular and plural names, and a command lookup
# timeout: 30
BASH_IT=$PWD/bash-it
BASH_IT_LOAD_PRIORITY_SEPARATOR=---
set -a
. ./bash-it/lib/log.bash
. ./bash-it/lib/utilities.bash
set +a
declare -p BASH_IT_LOG_LEVEL_ALL BASH_IT_LOG_LEVEL_TRACE
(BASH_IT_LOG_LEVEL_ALL=9) 2>/dev/null || echo "levels are read-only"
BASH_IT_LOG_LEVEL=$BASH_IT_LOG_LEVEL_ALL
_log_debug "a debug message"
_log_warning "a warning"
_log_error "an error"
BASH_IT_LOG_LEVEL=$BASH_IT_LOG_LEVEL_ERROR
_log_warning "not shown"
_log_error "still shown"
_bash-it-log-prefix-by-path "/x/plugins/available/base.plugin.bash"
echo "prefix: $BASH_IT_LOG_PREFIX"
_bash-it-log-prefix-by-path "/x/aliases/enabled/250---git.aliases.bash"
echo "prefix: $BASH_IT_LOG_PREFIX"
_bash-it-log-prefix-by-path "/x/vendor/github.com/z/lib.bash"
echo "prefix: $BASH_IT_LOG_PREFIX"
echo "name: $(_bash-it-get-component-name-from-path /x/plugins/available/node.plugin.bash)"
echo "name: $(_bash-it-get-component-name-from-path /x/enabled/150---git.aliases.bash)"
echo "type: $(_bash-it-get-component-type-from-path /x/aliases/available/git.aliases.bash)"
fruits=(apple banana "red pear")
_bash-it-array-contains-element "red pear" "${fruits[@]}" && echo "contains red pear"
_bash-it-array-contains-element pear "${fruits[@]}" || echo "no pear"
echo "dedup: $(_bash-it-array-dedup c a b a c | tr '\n' ' ')"
_bash-it-component-singularize plugins one
_bash-it-component-singularize aliases two
_bash-it-component-singularize completions
echo "singular: $one $two $_bash_it_component_singularize"
_bash-it-component-pluralize alias three
_bash-it-component-pluralize completion four
echo "plural: $three $four"
_command_exists sed && echo "sed exists"
_command_exists no-such-tool-zz || echo "no-such-tool-zz missing"
"$BASH" -c 'BASH_IT_LOG_LEVEL=6; _log_warning "from a child"; echo "child sees level $BASH_IT_LOG_LEVEL_WARNING"'
