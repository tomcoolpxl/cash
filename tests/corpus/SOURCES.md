# Where the corpus comes from

Each case names its source on its `# source:` line. The cases are test data, run under
cash and under Git Bash 5.3 with GNU sed and gawk (`run.ps1`); none of it ships in cash.
What each source allows, and the notices its terms ask to be kept with the copies:

| Cases | Source | Terms |
| --- | --- | --- |
| `bash/pbb-*` | [pure-bash-bible](https://github.com/dylanaraps/pure-bash-bible), Dylan Araps | MIT, below |
| `bash/psb-*` | [pure-sh-bible](https://github.com/dylanaraps/pure-sh-bible), Dylan Araps | MIT, the same notice as pure-bash-bible's |
| `bash/json-sh.sh` and `fixtures/JSON.sh` | [JSON.sh](https://github.com/dominictarr/JSON.sh), Dominic Tarr, unchanged | MIT or Apache License 2.0, at the user's choice; taken under MIT: Copyright (c) 2011 Dominic Tarr, under the MIT notice below |
| `bash/spark.sh` and `fixtures/spark` | [spark](https://github.com/holman/spark), Zach Holman, unchanged | MIT: Copyright (c) Zach Holman, https://zachholman.com, under the MIT notice below |
| `bash/getoptions.sh` and `fixtures/getoptions_*.sh` | [getoptions](https://github.com/ko1nksm/getoptions)' `lib/`, unchanged, and its `examples/basic.sh` definition | CC0 1.0, as each file's header says: no conditions |
| `bash/semver-tool.sh` and `fixtures/semver` | [semver-tool](https://github.com/fsaintjacques/semver-tool) 3.4.0's `src/semver`, unchanged | Apache License 2.0, as the script's SPDX line says; the text is in [`licenses/`](../../licenses/) (`*-Apache-2.0.txt`) |
| `awk/*` citing `onetrueawk` | [The One True Awk](https://github.com/onetrueawk/awk)'s `testdir`, the test programs of *The AWK Programming Language* | Lucent's permission notice, below |
| `sed/pement-*`, `awk/pement-*` | Eric Pement's one-liners, [sed](https://www.pement.org/sed/sed1line.txt) and [awk](https://www.pement.org/awk/awk1line.txt) | No licence stated; one-liners, cited by URL |
| `sed/*` citing `gnu.org` | The [GNU sed manual](https://www.gnu.org/software/sed/manual/)'s examples | GNU Free Documentation License 1.3 or later |
| `bash/pitfall-*`, `bash/faq73-*` | Greg's Wiki, [BashPitfalls](https://mywiki.wooledge.org/BashPitfalls) and [BashFAQ](https://mywiki.wooledge.org/BashFAQ) | No licence stated; short snippets, cited by URL. No more are taken from it |
| cases citing `stackoverflow.com` | Stack Overflow answers | CC BY-SA, as each answer's date gives it |
| cases citing `tests/…` or cash | cash's own differential scripts and tests | cash's licence |

New cases come from sources whose terms allow copying, and the table says which.

## pure-bash-bible

The notice JSON.sh and spark are under is the same, with their own copyright lines in the
table.

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
