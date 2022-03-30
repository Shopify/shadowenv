---
layout: default
keywords:
comments: false

title: Shadowlisp
description: Shadowenv programs are written in Shadowlisp, a simple Lisp dialect.

page_nav:
  prev:
    content: Getting Started
    url: /getting-started
  next:
    content: Best Practices
    url: /best-practices

---

Shadowlisp is a simple [Lisp](https://en.wikipedia.org/wiki/Lisp_(programming_language)) dialect. It
is a [Lisp-1](https://en.wikipedia.org/wiki/Common_Lisp#The_function_namespace), meaning that all
data types (functions and variables, notably) share a single namespace.

To express `nil`/`null`/no-value in Shadowlisp, use an empty list `()`. This will be important to
clear environment variables.

To give you a quick feel for what a relatively-complex Shadowlisp program might look like, here's a
snippet that uses more features than most code will have to:

<div class="example">
</div>
```scheme
(when-let ((rroot (env/get "RUBY_ROOT")))
 (env/remove-from-pathlist "PATH" (path-concat rroot "bin"))
 (when-let ((groot (env/get "GEM_ROOT")))
   (env/remove-from-pathlist "PATH" (path-concat groot "bin"))
   (env/remove-from-pathlist "GEM_PATH" groot))
 (when-let ((ghome (env/get "GEM_HOME")))
   (env/remove-from-pathlist "PATH" (path-concat ghome "bin"))
   (env/remove-from-pathlist "GEM_PATH" ghome)))
```

The remainder of this page is API documentation. The bread and butter of Shadowlisp is [Environment Manipulation](#environment-manipulation). The rest of the functions will only be useful in support of manipulating environment variables.

# Environment Manipulation

## `env/get`

`(env/get name)`

The simplest function to interact with the environment is `env/get`. It returns the current value of
the variable at the point in the script at which it's evaluated, not the initial value.

If the variable has no current value, `()` is returned instead of a String.

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to look up |

| Return Type | Description |
|---|---|
| `Option<String>` | Current value of variable, or `()` if unset |

```scheme
(env/get "PATH")  ; "/usr/bin:/usr/sbin:/bin:/sbin"
(env/get "DEBUG") ; () -- not set => null
```

## `env/set`

`(env/set name value)`

```scheme
(env/set "PATH" "/bin") ; ()
(env/get "PATH") ; "/bin"
```

The simplest form of mutation: `env/set` changes the value of an environment variable while
a Shadowenv is active. The previous value will be preserved so that it can be reactivated upon
deactivating the Shadowenv.

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to change |
| value | `Option<String>` | String to set the variable to, or `()` to unset it. |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

## `env/prepend-to-pathlist`

`(env/prepend-to-pathlist name entry)`

```scheme
(env/prepend-to-pathlist "PATH" "/opt/mytool/bin") ; ()
```

It's common to want to prepend an item to a `:`-delimited path (such as `PATH` or `MANPATH`).
`env/prepend-to-pathlist` does precisely this, first removing the item from the path if it was
already present, before prepending it to the front of the pathlist.

Strictly speaking, any variable can be treated as a pathlist by Shadowenv, but it only makes sense
to do this for variables that other tools expect to contain multiple items.

If there are no items in the list currently, `env/prepend-to-pathlist` will simply create the list with a single item.

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to change |
| entry | `String` | String to prepend |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

## `env/append-to-pathlist`

`(env/append-to-pathlist name entry)`

```scheme
(env/append-to-pathlist "PATH" "/opt/mytool/bin") ; ()
```

While less common than prepending, it's sometimes desirable to append an item to a `:`-delimited path (such as `PATH` or
`MANPATH`), to add it as a lower priority option.  `env/append-to-pathlist` does precisely this, first removing the item
from the path if it was already present, before appending it to the end of the pathlist.

Strictly speaking, any variable can be treated as a pathlist by Shadowenv, but it only makes sense
to do this for variables that other tools expect to contain multiple items.

If there are no items in the list currently, `env/append-to-pathlist` will simply create the list with a single item.

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to change |
| entry | `String` | String to append |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

## `env/remove-from-pathlist`

```scheme
(env/remove-from-pathlist name entry)
```

```scheme
(env/set "PATH" "/usr/bin:/opt/system-default-ruby/bin:/bin") ; ()
(env/remove-from-pathlist "PATH" "/opt/system-default-ruby/bin") ; ()
(env/get "PATH") ; "/usr/bin:/bin"
```

The counterpart to `env/prepend-to-pathlist`/`env/append-to-pathlist` is this, `env-remove-from-pathlist`. This won't be
as useful, since Shadowenv always takes care of its own deactivation, but you may occasionally want to
deactivate certain system-wide configuration upon entry into a Shadowenv.

If, after removing the indicated item from the specified pathlist, the variable becomes empty, it is
unset (equivalent to `(env/set var ())`).

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to change |
| entry | `String` | String to remove from pathlist |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

## `env/remove-from-pathlist-containing`

`(env/remove-from-pathlist-containing name substring)`

```scheme
(env/set "PATH" "/usr/bin:/root/.rvm/bin:/bin") ; ()
(env/remove-from-pathlist-containing "PATH" "/.rvm/") ; ()
(env/get "PATH") ; "/usr/bin:/bin"
```

A specialized version of `env/remove-from-pathlist`, `env-remove-from-pathlist-containing` will
remove any items from the pathlist which contain the provided value as a substring.

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to change |
| substring | `String` | Remove pathlist items containing this as a substring |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

# Utilities

## `path-concat`

`(path-concat [ strings ... ])`

```scheme
(path-concat "/" "usr" "bin") ; "/usr/bin"
```

```scheme
(when-let ((root (env/get "RUBY_ROOT")))
  (env/remove-from-pathlist (path-concat root "bin")))
```

It's occasionally useful to take a subdirectory of a path found from some other variable.
`path-concat` joins two or more strings (representing directories) with slashes.

This can be especially useful in the sort of usage shown to the right.

| Argument | Type | Description |
|---|---|---|
| `:rest` strings | `String` | Any number of strings to conjoin with `/` |

| Return Type | Description |
|---|---|
| `String` | Joined path |


## `expand-path`

`(expand-path path)`

```scheme
(expand-path "~/.gem") ; "/Users/you/.gem"
(expand-path "./bin") ; "/Users/you/src/project/bin"
```

`expand-path` resolves a path to a canonicalized path, resolving any symlinks, relative references
from the present working directory, and `~`.

| Argument | Type | Description |
|---|---|---|
| path | `String` | Path to expand |

| Return Type | Description |
|---|---|
| `String` | Expanded path |

## `provide`

`(provide feature [ version ])`


```scheme
(provide "ruby") ; activated shadowenv (ruby)
(provide "ruby" "2.3.7") ; activated shadowenv (ruby:2.3.7)
```

Allows a script to advertise to the user which feature it is providing, with an optional version number.

Multiple features with the same are allowed.

| Argument | Type | Description |
|---|---|---|
| feature | `String` | Name of the provided feature |
| version | `String` | Version of the provided feature. Optional. |

| Return Type | Description |
|---|---|
| `None` | Always returns `()` |

# Control Flow

## `when`

`(when condition [ then ... ])`

```scheme
(when (= 1 2) (env/set "NEVER" "happens")) ; ()
```

`when` tests a condition, evaluating the rest of its forms if and only if the condition is true.

| Argument | Type | Description |
|---|---|---|
| condition | `Expr` | If it evaluates to non-`()`, run *then* |
| `:rest` then | `Expr` | Evaluated if *condition* was true |

| Return Type | Description |
|---|---|
| `Any` | Whatever the return value of the last form in *then* was |


## `when-let`

```scheme
(when-let ( ( name expression ) )
  [ body ... ])
```

`when-let` evaluates some code if and only if a `let` binding, when run, assigns a non-`()` value
to the name.

```scheme
(env/set "RUBY_ROOT" ())
(when-let ((root (env/get "RUBY_ROOT")))
  (env/remove-from-pathlist "PATH" root)) ; not run

(env/set "RUBY_ROOT" "/opt/ruby-1")
(when-let ((root (env/get "RUBY_ROOT")))
  (env/remove-from-pathlist "PATH" root)) ; this time, it runs.
```

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name to assign |
| expression | `Any` | Value to assign to name |
| `:rest` body | `Expr` | Evaluated if *name* was assigned to something non-`()` |

| Return Type | Description |
|---|---|
| `Any` | Whatever the return value of the last form in *body* was |

## `if`

`(if condition then-case [ else-case ])`

The `if` operator evaluates its first argument, then evaluates only one of the
given branches, depending on the result. The "else" branch may be omitted,
in which case, `if` will yield `()` when the condition is `false`.

```scheme
(if (< a b)
  a
  b)
```

## `cond`

`(cond [ ( predicate branch ) ... ] [ ( else else-branch ) ] )`

The `cond` operator evaluates a series of predicates and executes the branch
for the first predicate which evaluates to true. The name `else` may be used for
the last case, as a catch-all branch.

```scheme
(cond
  ((< n 0) 'negative)
  ((> n 0) 'positive)
  (else    'zero))
```

## `do`

`(do [ expressions ... ])`

```scheme
(let ((a b) (c d))
  (do ; necessary in order to call both (f a) and (f c)
    (f a)
    (f c)))
```


The do operator executes multiple expressions and yields the value of the final expression. Useful
for forms like `let`, which only accept one form to evaluate.

| Argument | Type | Description |
|---|---|---|
| `:rest` expressions | `Expr` | Always evaluated |

| Return Type | Description |
|---|---|
| `Any` | Whatever the return value of the last form in *expressions* was |

# Logic

## `eq`

`(eq a b)`

Test whether two values are (weakly) equal. Shadowlisp does not support strict equality semantics.

## `ne`

`(ne a b)`

Test whether two values are (weakly) unequal. Shadowlisp does not support strict equality semantics.

## `not`

`(not a)`

```scheme
(not true) ; false
```

Boolean negation.

## `null`

`(null expr)`

Returns true if and only if the provided argument is `()`.

## `and`

`(and [ expression ... ])`

`and` evaluates its arguments, applying logical AND short-circuiting rules.

## `or`

`(or [ expression ... ])`

`or` evaluates its arguments, applying logical OR short-circuiting rules.

# Lists

## `append`

`(append list value)`

`append` appends a value to a list.

## `elt`

`(elt list n)`

`elt` returns the nth element of a list.

## `concat`

`(concat [ list ... ])`

`concat` concatenates each given list value.

## `join`

`(join sep [ list ... ])`

`join` joins together a series of lists using the first argument as separator.

## `first`

`(first list)`

`first` returns the first element of a list.

## `last`

`(last list)`

`last` returns the last element of a list.

## `tail`

`(tail list)`

`tail` returns all elements after the first element of a list.

## `list`

`(list [ expr ... ])`

`list` evaluates each of its arguments and return them as a list.

# Variable Binding

## `let`

`(let ( [ ( name expression ) ... ] ) body)`

The `let` operator defines a series of local bindings for the duration of the
execution of its body expression.

```scheme
(let ((a 1)
      (b 2))
  (+ a b))
```


<!--
from ketos, but unsupported/irrelevant
# define
# apply
# lambda
# macro
# new
# =
# second
# slice
# init
# format
# /=
# xor
# len
# id
# case
# is-instance
# reverse
# .
# .=
# type-of
# <
# >
# <=
# >=
# +
# -
# *
# zero
# max
# min
# struct
# path
# bytes
# /
# //
# rem
# <<
# >>
# bit&
# bit|
# bit^
# bit!
# is
# panic
# const
# set-module-doc
# string
# print
# println
# eprint
# eprintln
# export
# use
# denom
# fract
# numer
# rat
# recip
# abs
# ceil
# floor
# round
# trunc
# int
# chars
# float
# inf
# nan
-->
