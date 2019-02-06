# Mandatory for launch to `dev` users:

* Convert panic() usage into thoughtful error messages
* Find a better place to generate the key to
* rate-limit untrusted error message
* think about rate-limiting for other errors?

# Not strictly mandatory:

* Unify hook success and failure bail eprintln!(), there's duplication
* Implement expand-path
* Write man(1) documentation
* Write man(5) documentation
* Tests, tests, tests...
* Preserve ordering when re-inserting removed items in lists
* Add `shadowenv info` to show visual display of new/old items
