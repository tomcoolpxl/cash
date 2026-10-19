# Where the corpus comes from

Each case names its source on its `# source:` line. The cases are test data, run under
cash and under Git Bash 5.3 with GNU sed and gawk (`run.ps1`); none of it ships in cash.
What each source allows, and the notices its terms ask to be kept with the copies:

| Cases | Source | Terms |
| --- | --- | --- |
| `bash/pbb-*` | [pure-bash-bible](https://github.com/dylanaraps/pure-bash-bible), Dylan Araps | MIT, below |
| `bash/psb-*` | [pure-sh-bible](https://github.com/dylanaraps/pure-sh-bible), Dylan Araps | MIT, the same notice as pure-bash-bible's |
| `bash/json-sh.sh`, `bash/json-sh-run.sh` and `fixtures/JSON.sh` | [JSON.sh](https://github.com/dominictarr/JSON.sh), Dominic Tarr, unchanged | MIT or Apache License 2.0, at the user's choice; taken under MIT: Copyright (c) 2011 Dominic Tarr, under the MIT notice below |
| `bash/spark.sh` and `fixtures/spark` | [spark](https://github.com/holman/spark), Zach Holman, unchanged | MIT: Copyright (c) Zach Holman, https://zachholman.com, under the MIT notice below |
| `bash/getoptions.sh` and `fixtures/getoptions_*.sh` | [getoptions](https://github.com/ko1nksm/getoptions)' `lib/`, unchanged, and its `examples/basic.sh` definition | CC0 1.0, as each file's header says: no conditions |
| `bash/mo-templates.sh` and `fixtures/mo` | [mo](https://github.com/tests-always-included/mo) 3.1.0, unchanged | MIT style with a non-advertising clause and an acknowledgment, below |
| `bash/ansi-codes.sh` and `fixtures/ansi` | [ansi](https://github.com/fidian/ansi), © 2015 Tyler Akins, unchanged | the same terms as mo's, below |
| `bash/nvm-helpers.sh` and `fixtures/nvm.sh` | [nvm](https://github.com/nvm-sh/nvm)'s `nvm.sh`, unchanged | MIT: Copyright (c) 2010 Tim Caswell, Copyright (c) 2014 Jordan Harband, under the MIT notice below |
| `bash/semver-tool.sh` and `fixtures/semver` | [semver-tool](https://github.com/fsaintjacques/semver-tool) 3.4.0's `src/semver`, unchanged | Apache License 2.0, as the script's SPDX line says; the text is in [`licenses/`](../../licenses/) (`*-Apache-2.0.txt`) |
| `bash/wait-for-it.sh` and `fixtures/wait-for-it.sh` | [wait-for-it](https://github.com/vishnubob/wait-for-it), unchanged | MIT: Copyright (c) 2016 Giles Hall, under the MIT notice below |
| `bash/shflags.sh` and `fixtures/shflags` | [shFlags](https://github.com/kward/shflags), © 2008-2023 Kate Ward, unchanged | Apache License 2.0, as the script's header says; the text is in [`licenses/`](../../licenses/) (`*-Apache-2.0.txt`) |
| `bash/shunit2.sh` and `fixtures/shunit2` | [shUnit2](https://github.com/kward/shunit2), © 2008-2021 Kate Ward, unchanged | Apache License 2.0, as the script's header says; the text is in [`licenses/`](../../licenses/) (`*-Apache-2.0.txt`) |
| `bash/bashunit.sh` and `fixtures/bashunit/` | [bashunit](https://github.com/TypedDevs/bashunit) 0.52.0's runner and `src`, unchanged | MIT: Copyright (c) 2023 TypedDevs, under the MIT notice below |
| `bash/argbash.sh` and `fixtures/argbash/` | [Argbash](https://github.com/matejak/argbash) 2.11.0's `bin` and the generated scripts of `resources/examples`, unchanged | BSD 3-Clause: Copyright (c) 2015-2019, Matěj Týč, all rights reserved, under its `LICENSE`, copied beside the scripts |
| `bash/asdf.sh` and `fixtures/asdf/` | [asdf](https://github.com/asdf-vm/asdf) 0.15.0's `bin`, `lib`, `help.txt`, `version.txt` and `defaults`, unchanged | MIT: Copyright (c) 2014 Akash Manohar J, under the MIT notice below |
| `bash/bash-oo-framework.sh` and `fixtures/bash-oo-framework/` | [Bash Infinity](https://github.com/niieani/bash-oo-framework) at 0a9ea03, its `lib` and `example`, unchanged | MIT: Copyright (c) 2015 Bazyli Brzóska, under the MIT notice below |
| `bash/git-prompt.sh` and `fixtures/git-prompt/` | [Git](https://github.com/git/git) 2.56.0's `contrib/completion/git-prompt.sh`, unchanged | GNU GPL version 2, as Git is, its `COPYING` copied beside the script: Copyright (C) 2006,2007 Shawn O. Pearce |
| `bash/bash3boilerplate.sh` and `fixtures/bash3boilerplate/` | [BASH3 Boilerplate](https://github.com/kvz/bash3boilerplate) 2.8.0's `main.sh`, `example.sh` and `src` but `megamount.sh`, unchanged | MIT: Copyright (c) 2013 Kevin van Zonneveld and contributors, under its `LICENSE`, copied beside the scripts |
| `bash/sharness.sh` and `fixtures/sharness/` | [Sharness](https://github.com/felipec/sharness) 1.2.1's `sharness.sh` and `lib-sharness`, unchanged | GNU GPL version 2 or later, its `COPYING` copied beside the scripts: Copyright (c) 2011-2012 Mathias Lafeldt, Copyright (c) 2005-2012 Git project, Copyright (c) 2005-2012 Junio C Hamano, Copyright (c) 2019-2023 Felipe Contreras |
| `bash/shellspec.sh` and `fixtures/shellspec/` | [ShellSpec](https://github.com/shellspec/shellspec) 0.28.1's runner, `lib` and `libexec`, unchanged | MIT: Copyright (c) 2018 Koichi Nakashima, under the MIT notice below |
| `bash/git-subrepo.sh` and `fixtures/git-subrepo/` | [git-subrepo](https://github.com/ingydotnet/git-subrepo) 0.4.9's `lib`, unchanged, its `git-subrepo.d/bash+.bash` link replaced by the file it names (`ext/bashplus/lib/bash+.bash`) | MIT: Copyright (c) 2013-2020 Ingy döt Net, under the MIT notice below |
| `bash/bats-core.sh` and `fixtures/bats-core/` | [bats-core](https://github.com/bats-core/bats-core) 1.14.0's `bin`, `libexec` and `lib`, unchanged | MIT: Copyright (c) 2017 bats-core contributors, Copyright (c) 2014 Sam Stephenson, under the MIT notice below |
| `bash/z-jump.sh` and `fixtures/z.sh` | [z](https://github.com/rupa/z) 1.12, © 2009 rupa deadwyler, unchanged | WTFPL version 2, as the script's first line says: no conditions |
| `bash/bash-tpl.sh` and `fixtures/bash-tpl` | [bash-tpl](https://github.com/TekWizely/bash-tpl) 0.9.1, unchanged | MIT: Copyright (c) 2021 TekWizely & co-authors, under the MIT notice below |
| `bash/bash-yaml.sh` and `fixtures/yaml.sh` | [bash-yaml](https://github.com/jasperes/bash-yaml)'s `script/yaml.sh`, unchanged | MIT: Copyright (c) 2017 Jonathan Peres, under the MIT notice below |
| `bash/rbenv.sh` and `fixtures/rbenv/` | [rbenv](https://github.com/rbenv/rbenv) 1.3.2's `libexec`, unchanged | MIT: Copyright (c) 2013 Sam Stephenson, under the MIT notice below |
| `bash/git-flow.sh` and `fixtures/gitflow/` | [git-flow AVH Edition](https://github.com/petervanderdoes/gitflow-avh) 1.12.3's scripts, unchanged | BSD style: Copyright (c) 2012-2019 Peter van der Does, and Copyright 2010 Vincent Driessen, all rights reserved, under the BSD notices in its `LICENSE`, copied beside the scripts; `gitflow-shFlags` under the GNU LGPL 2.1, whose text that `LICENSE` holds |
| `bash/bashmarks.sh` and `fixtures/bashmarks.sh` | [bashmarks](https://github.com/huyng/bashmarks), © 2010 Huy Nguyen, unchanged | BSD 3-Clause, the notice in the script's header, kept with it |
| `bash/git-extras.sh`, `fixtures/git-summary` and `fixtures/git-effort` | [git-extras](https://github.com/tj/git-extras)' `bin/git-summary` and `bin/git-effort`, unchanged | MIT: Copyright (c) 2010 TJ Holowaychuk and Contributors, under the MIT notice below |
| `awk/*` citing `onetrueawk` | [The One True Awk](https://github.com/onetrueawk/awk)'s `testdir`, the test programs of *The AWK Programming Language* | Lucent's permission notice, below |
| `sed/pement-*`, `awk/pement-*` | Eric Pement's one-liners, [sed](https://www.pement.org/sed/sed1line.txt) and [awk](https://www.pement.org/awk/awk1line.txt) | No licence stated; one-liners, cited by URL |
| `sed/*` citing `gnu.org` | The [GNU sed manual](https://www.gnu.org/software/sed/manual/)'s examples | GNU Free Documentation License 1.3 or later |
| `bash/pitfall-*`, `bash/faq73-*` | Greg's Wiki, [BashPitfalls](https://mywiki.wooledge.org/BashPitfalls) and [BashFAQ](https://mywiki.wooledge.org/BashFAQ) | No licence stated; short snippets, cited by URL. No more are taken from it |
| cases citing `stackoverflow.com` | Stack Overflow answers | CC BY-SA, as each answer's date gives it |
| cases citing `tests/…` or cash | cash's own differential scripts and tests | cash's licence |

New cases come from sources whose terms allow copying, and the table says which.

## pure-bash-bible

The notice JSON.sh, spark and nvm are under is the same, with their own copyright lines in
the table.

The MIT License (MIT)

Copyright (c) 2018 Dylan Araps

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## mo and ansi

This product includes software developed by contributors.

Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
to whom the Software is furnished to do so, subject to the following conditions:

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

Except as contained in this notice, the name(s) of the above copyright holders shall not
be used in advertising or otherwise to promote the sale, use or other dealings in this
Software without prior written authorization.

The end-user documentation included with the redistribution, if any, must include the
following acknowledgment: "This product includes software developed by contributors", in
the same place and form as other third-party acknowledgments. Alternately, this
acknowledgment may appear in the software itself, in the same form and location as other
such third-party acknowledgments.

## The One True Awk

Copyright (C) Lucent Technologies 1997
All Rights Reserved

Permission to use, copy, modify, and distribute this software and
its documentation for any purpose and without fee is hereby
granted, provided that the above copyright notice appear in all
copies and that both that the copyright notice and this
permission notice and warranty disclaimer appear in supporting
documentation, and that the name Lucent Technologies or any of
its entities not be used in advertising or publicity pertaining
to distribution of the software without specific, written prior
permission.

LUCENT DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS.
IN NO EVENT SHALL LUCENT OR ANY OF ITS ENTITIES BE LIABLE FOR ANY
SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER
IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.
