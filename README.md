# Shadowenv

Shadowenv provides a way to perform a set of manipulations to the process environment upon entering
a directory in a shell. These manipulations are reversed when leaving the directory, and there is
some limited ability to make the manipulations dynamic.

![shadowenv in action](https://burkelibbey.s3.amazonaws.com/shadowenv.gif)

In order to use shadowenv, add a line to your shell profile (`.zshrc`, `.bash_profile`, or
`config.fish`) reading:

```bash
eval "$(shadowenv init bash)" # for bash
eval "$(shadowenv init zsh)"  # for zsh
shadowenv init fish | source  # for fish
```

With this code loaded, upon entering a directory containing a `.shadowenv.d` directory,
any `*.lisp` files in that directory will be executed and you will see "activated shadowenv." in your
shell.

The syntax for the `.shadowenv.d/*.lisp` files is [Shadowlisp](https://shopify.github.io/shadowenv/),
a minimal [Scheme-like](https://en.wikipedia.org/wiki/Scheme_(programming_language)) language.
Unlike other tools like [direnv](https://direnv.net/), this has the interesting property of allowing
us to do things like simulate `chruby reset` upon entry into a directory without the user having
`chruby` installed (and undo these changes to the environment when `cd`'ing back out):

```scheme
(provide "ruby")

(when-let ((ruby-root (env/get "RUBY_ROOT")))
  (env/remove-from-pathlist "PATH" (path-concat ruby-root "bin"))
  (when-let ((gem-root (env/get "GEM_ROOT")))
    (env/remove-from-pathlist "PATH" (path-concat gem-root "bin"))
    (env/remove-from-pathlist "GEM_PATH" gem-root))
  (when-let ((gem-home (env/get "GEM_HOME")))
    (env/remove-from-pathlist "PATH" (path-concat gem-home "bin"))
    (env/remove-from-pathlist "GEM_PATH" gem-home)))
```

The intention isn't really for users to write these files directly, nor to commit them to
repositories , but for other tool authors to generate configuration on the user's machine.

## `.shadowenv.d`

The `.shadowenv.d` directory will generally exist at the root of your repository (in the same
directory as `.git`.

We *strongly* recommend creating `gitignore`'ing everything under `.shadowenv.d` (`echo '*' > .shadowenv.d/.gitignore`).

A `.shadowenv.d` will contain any number of `*.lisp` files. These are evaluated in the order in which
the OS returns when reading the directory: generally alphabetically. We *strongly* recommend using
a prefix like `090_something.lisp` to make it easy to maintain ordering.

`.shadowenv.d` will also contain a `.trust-<fingerprint>` file if it has been marked as trusted. (see
the trust section).

## Language

See https://shopify.github.io/shadowenv/ for Shadowlisp documentation.

## Integrations

Shadowenv has plugins for multiple editors and/or IDEs:

* [shadowenv.vim](https://github.com/Shopify/shadowenv.vim)
* [shadowenv.el](https://github.com/Shopify/shadowenv.el)
* [vscode-shadowenv](https://github.com/Shopify/vscode-shadowenv)
* [atom-shadowenv](https://github.com/Shopify/atom-shadowenv)
* [sublime-shadowenv](https://github.com/Shopify/sublime-shadowenv)

## Trust

If you `cd` into a directory containing `.shadowenv.d/*.lisp` files, they will not be run and you
will see a message indicating so. This is for security reasons: we don't want to enable random
stuff downloaded from the internet to modify your `PATH`, for example.

You can run `shadowenv trust` to mark a directory as trusted.

Technically, running `shadowenv trust` will create a file at `.shadowenv.d/.trust-<fingerprint>`,
indicating that it's okay for `shadowenv` to run this code. The `.shadowenv.d/.trust-*` file contains
a cryptographic signature of the directory path. The key is generated the first time `shadowenv` is
run, and the fingerprint is an identifier for the key.
