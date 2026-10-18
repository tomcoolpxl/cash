# source: https://github.com/tests-always-included/mo/blob/master/mo (3.1.0, the fixture `mo`, unchanged)
# desc: a real template engine in Bash, sourced: variables, sections over arrays, inverted sections, comments, escaping, associative arrays and partials
. ./mo
NAME=World
mo <<<'Hello {{NAME}}!'
ITEMS=(apple banana cherry)
mo <<<'{{#ITEMS}}- {{.}}
{{/ITEMS}}done'
EMPTY=
mo <<<'{{^EMPTY}}empty is false{{/EMPTY}}{{#NAME}}, name is set{{/NAME}}'
mo <<<'{{! a comment }}after the comment'
HTML='<b>&"x"</b>'
mo <<<'{{HTML}} and {{{HTML}}} and {{&HTML}}'
declare -A PERSON=([first]=Ada [last]=Lovelace)
mo <<<'{{PERSON.first}} {{PERSON.last}}'
printf 'the partial says {{NAME}}\n' >part.mo
mo <<<'before
{{> part.mo}}
after'
NUMS=(1 2 3)
mo <<<'{{#NUMS}}[{{.}}]{{/NUMS}} {{^NUMS}}none{{/NUMS}}'
NOTHING=()
mo <<<'{{#NOTHING}}never{{/NOTHING}}{{^NOTHING}}no items{{/NOTHING}}'
mo <<<'{{=<% %>=}}<% NAME %> with other delimiters'
printf 'From a file: {{NAME}}\n' >template.mo
mo template.mo
mo -u <<<'{{NO_SUCH_VARIABLE}}' 2>/dev/null; echo "status with -u: $?"
mo <<<'standalone:
  {{#NAME}}
  inside
  {{/NAME}}
end'
