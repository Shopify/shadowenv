---
title: "Shadowlisp"
date: 2019-02-08T13:40:07-05:00
draft: true
---

# Shadowlisp

Shadowlisp is a simple [Lisp](https://en.wikipedia.org/wiki/Lisp_(programming_language)) dialect. It
is a [Lisp-1](https://en.wikipedia.org/wiki/Common_Lisp#The_function_namespace), meaning that all
data types (functions and variables, notably) share a single namespace.

```lisp
(env/set "DEBUG" ()) ; () is the null value
```

To express `nil`/`null`/no-value in Shadowlisp, use an empty list `()`. This will be important to
clear environment variables.

```lisp
(let ((a b) (c d))
  (do ; necessary in order to call both (f a) and (f c)
    (f a)
    (f c)))
```

In many modern Lisp dialects, forms like like `let` take a variable number of body forms (an
implicit `do`). This is not the case in Shadowlisp, and you will find yourself having to explicitly
use `do` from time to time.

The remainder of this page is API documentation. The bread and butter of Shadowlisp is the functions
beginning with `env/*`. The rest of the API is only useful in support of manipulating environment
variables.

# env/get

```lisp
(env/get name)
```

The simplest function to interact with the environment is `env/get`. It returns the current value of
the variable at the point in the script at which it's evaluated, not the initial value.

```lisp
(env/get "PATH") ; "/usr/bin:/usr/sbin:/bin:/sbin"
```

| Argument | Type | Description |
|---|---|---|
| name | `String` | Name of environment variable to look up |

> If the variable has no current value, `()` is returned instead of a String.

```lisp
(env/get "DEBUG") ; () -- not set => null
```

| Return Type | Description |
|---|---|
| `Option<String>` | Current value of variable, or `()` if unset |


# env/set

```lisp
(env/set name value)
```

```lisp
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

# env/prepend-to-pathlist

```lisp
(env/prepend-to-pathlist name entry)
```

```lisp
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

# env/remove-from-pathlist

```lisp
(env/remove-from-pathlist name entry)
```

```lisp
(env/set "PATH" "/usr/bin:/opt/system-default-ruby/bin:/bin") ; ()
(env/remove-from-pathlist "PATH" "/opt/system-default-ruby/bin") ; ()
(env/get "PATH") ; "/usr/bin:/bin"
```

The counterpart to `env/prepend-to-pathlist` is this, `env-remove-from-pathlist`. This won't be as
useful, since Shadowenv always takes care of its own deactivation, but you may occasionally want to
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

# env/remove-from-pathlist-containing

```lisp
(env/remove-from-pathlist-containing name substring)
```

```lisp
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

# path-concat

```lisp
(path-concat [ strings ... ])
```

```lisp
(path-concat "/" "usr" "bin") ; "/usr/bin"
```

```lisp
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

# expand-path

```lisp
(expand-path path)
```

```lisp
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

# when

```lisp
(when condition
  [ then ... ])
```

```lisp
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


# when-let

```lisp
(when-let ( ( name expression ) )
  [ body ... ])
```

`when-let` evaluates a some code if and only if a `let` binding, when run, assigns a non-`()` value
to the name.

```lisp
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

# do

```lisp
(do [ expressions ... ])
```

```lisp
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

# eq

```lisp
(eq a b)
```

Test whether two values are (weakly) equal. Shadowlisp does not support strict equality semantics.

# ne

```lisp
(ne a b)
```

Test whether two values are (weakly) unequal. Shadowlisp does not support strict equality semantics.

# not

```lisp
(not a)
```

```lisp
(not true) ; false
```

Boolean negation.

# append

```lisp
(append list value)
```

`append` appends a value to a list.

# elt

```lisp
(elt list n)
```

`elt` returns the nth element of a list.

# concat

```lisp
(concat [ list ... ])
```

`concat` concatenates each given list value.

# join

```lisp
(join sep [ list ... ])
```

`join` joins together a series of lists using the first argument as separator.

# first

```lisp
(first list)
```

`first` returns the first element of a list.

# last

```lisp
(last list)
```

`last` returns the last element of a list.

# tail

```lisp
(tail list)
```

`tail` returns all elements after the first element of a list.

# list

```lisp
(list [ expr ... ])
```

`list` evaluates each of its arguments and return them as a list.

# null

```lisp
(null expr)
```

Returns true if and only if the provided argument is `()`.

# apply

```lisp
(apply function [ arguments ... ] argument-list)
```

The `apply` operator calls a function with a given series of arguments.
The argument list consists of any positional arguments except for the last
argument to `apply`, plus the final, required list argument, which is
concatenated to positional arguments.

```lisp
(apply + 1 2 3 '(4 5 6))
```

# let

```lisp
(let ( [ ( name expression ) ... ] ) body)
```

The `let` operator defines a series of local bindings for the duration of the
execution of its body expression.

```lisp
(let ((a 1)
      (b 2))
  (+ a b))
```

# define

```lisp
(define name expression)

(define (name [ arguments ...
                [ :optional arguments ... ]
                [ :key arguments ... ]
                [ :rest rest-argument ]
                ] ) expression)
```

The `define` operator adds a value or compiled function to the global scope.

```lisp
; Associates the global name `foo` with the value `123`.
(define foo 123)

; Associates the global name `bar` with a function that returns twice its input.
(define (bar a) (* a 2))
```

When defining a function, if the keyword `:optional` is present in the argument
list, all following arguments will be optional. If the keyword `:key` is present,
all following arguments will be optional keyword arguments. If the keyword
`:rest` is present, the following name will contain any free arguments remaining.

Optional and keyword arguments may be omitted when calling a function.
If an optional or keyword value is not supplied its value will be `()`.
A default value can be given when the function is defined.

```lisp
; Defines a function taking an optional argument, a.
(define (foo :optional a) a)

; Calls foo with no arguments. The value of `a` will be `()`.
(foo)
; Calls foo with arguments. The value of `a` will be `123`.
(foo 123)

; Defines a function taking an optional keyword argument, a,
; whose value defaults to `1`.
(define (bar :key (a 1)) a)

; Calls bar with no arguments. The value of `a` will be `1`.
(bar)
; Calls bar with keyword argument. The value of `a` will be `2`.
(bar :a 2)
```

# macro

```lisp
(macro (name [ arguments ... ]) expression)
```

The `macro` operator defines a compile-time macro. A macro behaves in all
respects as any other function, except that it is executed at compile time
and is expected to return code which is then further compiled.

# if

```lisp
(if condition
  then-case
  [ else-case ])
```

The `if` operator evaluates its first argument, then evaluates only one of the
given branches, depending on the result. The "else" branch may be omitted,
in which case, `if` will yield `()` when the condition is `false`.

```lisp
(if (< a b)
  a
  b)
```

# and

```lisp
(and [ expression ... ])
```

`and` evaluates its arguments, applying logical AND short-circuiting rules.

# or

```lisp
(or [ expression ... ])
```

`or` evaluates its arguments, applying logical OR short-circuiting rules.

# cond

```lisp
(cond
  [ ( predicate branch ) ... ]
  [ ( else else-branch ) ] )
```

The `cond` operator evaluates a series of predicates and executes the branch
for the first predicate which evaluates true. The name `else` may be used for
the last case, as a catch-all branch.

```lisp
(cond
  ((< n 0) 'negative)
  ((> n 0) 'positive)
  (else    'zero))
```

# lambda

```lisp
(lambda ( [ arguments ... ] ) expression)
```

The `lambda` operator creates a function which may enclose one or more local
value bindings from the surrounding scope.

```lisp
(define (adder a)
  (lambda (b) (+ a b)))
```

<!--
from ketos, but unsupported/irrelevant
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
