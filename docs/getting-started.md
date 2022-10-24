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

**On macOS, you can `brew install shadowenv` or [download the binary from
GitHub](https://github.com/Shopify/shadowenv/releases) and drop it somewhere on your path.** If this
build isn't right for your architecture, the only other option for now is to build it manually from
source.

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
  <p>We're not in principle against supporting other shells, but for now Shadowenv only works with bash, zsh, and fish.</p>
</div>

Shadowenv relies on a shell hook to make changes when you change directories. In order to use it,
add a line to your shell profile (`.zshrc`, `.bash_profile`, `config.fish`, or `.xonshrc`) reading:

```bash
# (only add one of these!)
eval "$(shadowenv init bash)"  # for bash
eval "$(shadowenv init zsh)"   # for zsh
shadowenv init fish | source   # for fish
execx($(shadowenv init xonsh)) # for xonsh
```

Make sure to restart your shell after adding this.

# A Quick Demo

Shadowenv constantly scans your current directory, and all of its parents, for a directory named
`.shadowenv.d`. The nearest one wins, just like if you have nested git repositories.

When a `.shadowenv.d` directory is found, Shadowenv first checks that you've [Trusted]({% if jekyll.environment == 'production' %}{{ site.doks.baseurl }}{% endif %}/trust) it.
Then, it looks for any files ending with `.lisp` in that directory, and runs them as
[Shadowlisp]({% if jekyll.environment == 'production' %}{{ site.doks.baseurl }}{% endif %}/shadowlisp).

Here's an example of what a Shadowlisp file might look like:

<div class="example"> <code>.shadowenv.d/500_default.lisp</code></div>
```scheme
(env/set "DEBUG" "1")
(env/prepend-to-pathlist "PATH" "./bin")
```

If you try creating an empty `.shadowenv.d`, and then adding that content to a `*.lisp` file inside
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

Those are the basics! There's a bit [more you can do with Shadowlisp]({% if jekyll.environment == 'production' %}{{ site.doks.baseurl }}{% endif %}/shadowlisp), and we have some
[suggestions]({% if jekyll.environment == 'production' %}{{ site.doks.baseurl }}{% endif %}/best-practices) for how to actually use Shadowenv in an organization.

### Add to your editor or IDE

Depending on you use your editor, you may find it helpful to have the same environment variable
settings as in your shell. We have plugins for most common editors, and writing new plugins is
relatively straightforward.

Check out the [Integration]({% if jekyll.environment == 'production' %}{{ site.doks.baseurl }}{% endif %}/integration) page for more information and a list of available plugins.

# Usage in Scripts

Sometimes you may want a Shadowenv loaded in a non-interactive script. This is what `shadowenv exec`
is for. When you run `shadowenv exec`, Shadowenv will load the Shadowlisp from the current directory
and execute the specified program. For example, imagine a directory with a Shadowenv that puts
`nginx` on the `PATH`:

```bash
#!/bin/sh
cd /path/to/thing/with/dot-shadowenv

shadowenv exec -- nginx -g 'daemon off;'
```

In this case, we didn't need to load the Shadowenv shell hook, as `shadowenv exec` loaded the
environment before executing nginx.
