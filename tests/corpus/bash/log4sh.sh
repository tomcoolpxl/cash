# source: https://github.com/kward/log4sh/tree/2ca6e1e (`log4sh`, `examples/hello_world`, `examples/log4sh.properties.ex1` and LICENSE, unchanged, as the fixture directory `log4sh`)
# desc: log4sh at 2ca6e1e, the log4j-like logging framework for shell scripts, a single file of 3700 lines: its hello world example; the library configured in code with a ConsoleAppender and FileAppenders to a file and to standard error, PatternLayouts of the priority, the category, the thread, the file name, an MDC variable and a literal percent, SimpleLayout, the levels from TRACE to OFF, an appender's threshold, a message piped in, several appenders at once; and configured from a properties file, its example with a pattern without the clock; the patterns that print the time (`%d`, `%r`) are left out
# timeout: 300
cd log4sh || exit 1
echo "== hello world"
# The default pattern starts with the seconds since the script started, `%-4r`.
sh ./hello_world | sed 's/^[0-9]* /R /'; echo "status: ${PIPESTATUS[0]}"
echo "== no configuration"
LOG4SH_CONFIGURATION='none' . ./log4sh
log4sh_resetConfiguration
logger_setLevel INFO
logger_addAppender stdout
appender_setType stdout ConsoleAppender
appender_setLayout stdout PatternLayout
appender_setPattern stdout '%p [%t] %c - %m'
appender_activateOptions stdout
logger_info 'Hello, world'
logger_debug 'not shown: below INFO'
logger_warn 'a warning'
echo "== every level"
for level in TRACE DEBUG INFO WARN ERROR FATAL OFF; do
  logger_setLevel "$level"
  printf 'at %s:' "$level"
  for call in trace debug info warn error fatal; do
    out=$("logger_$call" "$call" 2>&1)
    [ -n "$out" ] && printf ' %s' "$call"
  done
  echo
done
logger_setLevel INFO
echo "== a file appender and a threshold"
logger_addAppender logfile
appender_setType logfile FileAppender
appender_file_setFile logfile ../out.log
appender_setLayout logfile PatternLayout
appender_setPattern logfile '[%p] %m'
appender_setLevel logfile WARN
appender_activateOptions logfile
logger_info 'info: console only'
logger_error 'error: console and file'
echo "-- the file"
cat ../out.log
echo "== standard error, the MDC and a percent"
logger_addAppender err
appender_setType err FileAppender
appender_file_setFile err STDERR
appender_setLayout err PatternLayout
client=42
appender_setPattern err 'err %X{client} 100%% %m'
appender_activateOptions err
logger_setThreadName worker
logger_fatal 'to all three' 2>&1
echo "-- piped"
printf 'line one\nline two\n' | logger_warn 2> /dev/null
echo "== simple layout"
appender_setLayout stdout SimpleLayout
appender_activateOptions stdout
logger_info 'simple' 2> /dev/null
echo "== a properties file"
sed 's/%-4r \[%t\] //' log4sh.properties.ex1 > ../log4sh.properties
cd .. && printf '%s\n' 'LOG4SH_CONFIGURATION=log4sh.properties . ./log4sh/log4sh' 'logger_info "from the properties file"' 'logger_debug "not shown"' 'logger_error "an error"' 'appender_setPattern A1 "%F: %p %m"' 'appender_activateOptions A1' 'logger_info "named by the file"' > props.sh
bash props.sh; echo "status: $?"
