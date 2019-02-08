---
title: "Shadowlisp"
date: 2019-02-08T13:40:07-05:00
draft: true
---

# Shadowlisp

Shadowlisp is a simple [Lisp](https://en.wikipedia.org/wiki/Lisp_(programming_language)) dialect. It
is a [Lisp-1](https://en.wikipedia.org/wiki/Common_Lisp#The_function_namespace), meaning that all
data types (functions and variables, notably) share a single namespace.

To express `nil`/`null`/no-value in Shadowlisp, use an empty list `()`. This will be important to
clear environment variables (for example, with `(env/set "DEBUG" ())`.

```scheme
(let ((a b) (c d))
  (do ; necessary in order to call both (f a) and (f c)
    (f a)
    (f c)))
```

In many modern Lisp dialects, forms like like `let` take a variable number of body forms (an
implicit `do`). This is not the case in Shadowlisp, and you will find yourself having to explicitly
use `do` from time to time.

The remainder of this page is API documentation. The bread and butter of Shadowlisp is the
*Environment Interaction* section, since manipulating the environment is the only possible
side-effect in Shadowlisp. The rest of the API is only useful in support of setting environment
variables.

# env/get

> asdf?

```scheme
(env/get "PATH") ; "/usr/bin:/usr/sbin:/bin:/sbin"
(env/set "PATH" "/bin") ; ()
(env/get "PATH") ; "/bin"
```

The simplest function to interact with the environment is `env/get`. It returns the current value of
the variable at the point in the script at which it's evaluated, not the initial value.

