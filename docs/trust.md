---
layout: default
keywords:
comments: false

title: Trust
description: How shadowenv manages authorization

page_nav:
  prev:
    content: Best Practices
    url: /best-practices
  next:
    content: Integration
    url: /integration


---

**Short version: Run `shadowenv trust` to tell Shadowenv that it's ok to run from the directory
you're in.**

Because of how shadowenv works (loading code from whichever directory you `cd` into), it's important
to have some concept of trusting shadowlisp code before it's allowed to run. Shadowenv does this in
a fairly lightweight way, by marking an entire directory as trusted, and allowing any code to be run
from within it forever. The main case we're trying to defend against is downloading a random tarball
and having it modify your environment upon `cd`'ing into it.

The first time Shadowenv runs, it will create a cryptographic signing key at
`~/.config/shadowenv/trust-key`. When you `cd` into a directory with a `.shadowenv.d` (or create
one), you'll see an error message:

```
shadowenv failure: directory contains untrusted shadowenv program: shadowenv help trust to learn more.
```

If you run `shadowenv trust`, a new file will be created at `.shadowenv.d/trust-<fingerprint>`,
where `<fingerprint>` is derived from your key. The contents of the file is a signature of the
directory in which the `.shadowenv.d` lives. Before loading any code, shadowenv verifies this
signature.

This signature will become invalid if you move the directory, and it does resolve symbolic links
before signing.

## Multiple Shadowenvs in the file path
Shadowenv loads envs from all ancestors of the current directory. The loading is an all-or-nothing approach:
- If all envs that lead up to the current directory are trusted, then shadowenv will load everything.
- If not, then nothing will be loaded.

Shadowenv will let you know which environments are untrusted in the path:
```
shadowenv failure: The following directories contain untrusted shadowenv programs (see shadowenv help trust to learn more):
/path/to/env/a/b/.shadowenv.d
/path/to/env/a/.shadowenv.d
```

Note that Shadowenv applies envs from higher up the file system tree first.
