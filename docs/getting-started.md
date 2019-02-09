---
layout: default
keywords:
comments: false

title: Getting Started
description: How to install shadowenv and take it for a spin

page_nav:
  prev:
    content: Home
    url: /
  next:
    content: Shadowlisp API
    url: /shadowlisp

---

# Installation

<div class="callout callout--danger">
  <p><strong>Alpha Software</strong></p>
  <p>While you can expect Shadowenv to more or less work, it's very new, and we're still ironing out a lot of bugs. Caveat Emptor.</p>
</div>

Shadowenv isn't yet packaged in any package managers. For the moment, you'll need to build it from source.

## Building from Source

### Clone the git repo

`dev clone shadowenv` if you work at Shopify, or:

```bash
git clone https://github.com/Shopify/shadowenv
cd shadowenv
```

### Install Rust

`dev up` if you work at Shopify, or:

```bash
# Install rustup
curl https://sh.rustup.rs -sSf | sh

# Install the version of rust that we need
rustup switch nightly-2018-12-26k
```

### Build and Install

```bash
cargo build --release
```

This will generate an artifact at `target/release/shadowenv`. You can copy this wherever you like on
your path, for example:

```bash
cp target/release/shadowenv /usr/local/bin/shadowenv
```

### Add to your shell profile

<div class="callout callout--info">
  <p><strong>Shell support</strong></p>
  <p>We're not in principle against supporting other shells, but for not Shadowenv only works with bash, zsh, and fish.</p>
</div>

Shadowenv relies on a shell hook to make changes when you change directories. In order to use it,
add a line to your shell profile (`.zshrc`, `.bash_profile`, or `config.fish`) reading:

```bash
# (only add one of these!)
eval "$(shadowenv init bash)" # for bash
eval "$(shadowenv init zsh)"  # for zsh
shadowenv init fish | source  # for fish
```

Make sure to restart your shell after adding this.

# A Quick Demo

Shadowenv constantly scans your current directory, and all of its parents, for a directory named
`.shadowenv.d`. The nearest one wins, just like if you have nested git repositories.

When a `.shadowenv.d` directory is found, Shadowenv first checks that you've [Trusted](/trust) it.
Then, it looks for any files ending with `.scm` in that directory, and runs them as
[Shadowlisp](/shadowlisp).

Here's an example of what a Shadowlisp file might look like:

<div class="example"> <code>.shadowenv.d/500_default.scm</code></div>
```scheme
(env/set "DEBUG" "1")
(env/prepend-to-pathlist "PATH" "./bin")
```

If you try creating an empty `.shadowenv.d`, and then adding that content to a `*.scm` file inside
it, you'll get an error back about an "untrusted" shadowenv. In order to fix this, run:

```bash
shadowenv trust
```

This will mark the directory as trusted, telling shadowenv that it can safely run any shadowlisp
programs it finds inside. You should see "shadowenv activated." in your terminal. Then, if you `echo
$DEBUG`, you will see "1" printed, because the script set "DEBUG" to "1".

Next, `cd ..` to get out of the directory containing the shadowenv. You will see "deactivated
shadowenv." printed automatically. If you `echo $DEBUG`, you'll see whatever value you had prior to
creating the Shadowenv: most likely no value.

Those are the basics! There's a bit [more you can do with Shadowlisp](/shadowlisp), and we have some
[suggestions](/best-practices) for how to actually use Shadowenv in an organization.
