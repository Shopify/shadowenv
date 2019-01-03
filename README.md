# Shadowenv [wip]

**This is already out of date, but will be brought up to date and finished soon.**

Shadowenv provides a way to perform a set of manipulations to the process environment upon entering
a directory in a shell. These manipulations are reversed when leaving the directory, and there is
some limited ability to make the manipulations dynamic.

![shadowenv in action](https://burkelibbey.s3.amazonaws.com/shadowenv.gif)

In order to use Shadowenv, you must either `source shadowenv.sh` (for bash or zsh) or `source
shadowenv.fish` (for fish). Then, upon entering a directory containing a `.shadowenv` file, the
program will be executed and you will see "activated shadowenv." in your shell.

The syntax for the `.shadowenv` file is a minimal
[Scheme-like](https://en.wikipedia.org/wiki/Scheme_(programming_language)) language. This has
the interesting property of allowing us to do things like simulate `chruby reset` upon entry into
a directory without the user having `chruby` installed (and undo these changes to the environment
when `cd`'ing back out):

```scheme
(when-let ((ruby-root (env/get "RUBY_ROOT")))
  (env/remove-from-pathlist "PATH" (path-concat ruby-root "bin"))
  (when-let ((gem-root (env/get "GEM_ROOT")))
    (env/remove-from-pathlist "PATH" (path-concat gem-root "bin"))
    (env/remove-from-pathlist "GEM_PATH", gem-root))
  (when-let ((gem-home (env/get "GEM_HOME")))
    (env/remove-from-pathlist "PATH" (path-concat gem-home "bin"))
    (env/remove-from-pathlist "GEM_PATH", gem-home)))
```

The intention isn't really for users to write these files directly, nor to commit them to
repositories, but for other tool authors to generate configuration on the user's machine. Before
v1.0, we'll likely add `.shadowenv.d/*` as a concept.

## Language

The three most important functions, and for simple use-cases the only functions you will need, are:

* `env/set`
* `env/prepend-to-pathlist`
* `env/remove-from-pathlist`

#### `env/set <key> <value>`

```scheme
;; sets an environment variable to a new value, or erases it if passed `()`.
;; Example:
(env/set "RUBYOPT" ()) ; unset RUBYOPT
(env/set "DEBUG" "1")  ; export DEBUG=1
```

#### `env/prepend-to-pathlist <key> <value>`

```scheme
;; prepends an item to a ':'-delimited list
;; Example:
(env/prepend-to-pathlist "PATH" "/opt/bin")  ; export PATH="/opt/bin:${PATH}"
```

#### `env/remove-from-pathlist <key> <value>`

```scheme
;; removes an item from a ':'-delimited list, unsetting the list if it's the last item.
;; Example:
(env/remove-from-pathlist "PATH" "/usr/local/bin")
```

### Other functions

There are a number of other functions available, largely driven by the things we have needed to
implement our own use-cases. Take a look at:

* The [standard library
  definition](https://github.com/Shopify/shadowenv/blob/master/lib/shadowenv/lang/lib.rb); or
* [Shadowenv's own `.shadowenv`](https://github.com/Shopify/shadowenv/blob/master/.shadowenv).

## Usability

Shadowenv is not yet ready for use as anything other than a (dangerous) toy. The primary thing we
have to implement before it can be safely used is a concept of which directories are trusted to
contain a `.shadowenv` (imagine downloading a random tarball containing a shadowenv that would
override `PATH` to make some standard utility behave differently!)
