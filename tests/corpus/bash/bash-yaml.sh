# source: https://github.com/jasperes/bash-yaml/blob/master/script/yaml.sh (the fixture `yaml.sh`, unchanged)
# desc: a real YAML reader sourced: sed with a newline in its replacement and a control character as field separator, two awk passes building names from indentation, lists as arrays, quotes, comments, variables made with eval and unset again
printf '%s\n' \
  '# a comment' \
  'name: cash' \
  'version: "1.32.0"' \
  'build:' \
  '  jobs: 8 # inline comment' \
  '  profile: release' \
  '  flags:' \
  '    - --locked' \
  '    - --offline' \
  'authors:' \
  '  - name: Ann' \
  '    role: dev' \
  '  - name: Bob' \
  '    role: ops' \
  'with-dash.key: yes' > config.yml
. ./yaml.sh
parse_yaml config.yml
echo ---
parse_yaml config.yml conf_ | sort
echo ---
create_variables config.yml
echo "$name $version, jobs $build_jobs, ${#build_flags[@]} flags: ${build_flags[*]}"
echo "authors: ${authors__name[*]} as ${authors__role[*]}"
echo "with_dash_key=$with_dash_key"
unset_variables "$(parse_yaml config.yml)"
echo "after unset: [${name-unset}] [${build_flags[*]-unset}]"
bash ./yaml.sh config.yml cli_ | head -3
