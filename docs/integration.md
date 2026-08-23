---
layout: default
keywords:
comments: false

title: Integration
description: Integrations for various editors, etc., and how to write your own

page_nav:
  prev:
    content: Trust
    url: /trust

---

If you've followed the setup instructions, you have your shell configured to load the Shadowenv from
a directory when you move into it, but this doesn't automatically make IDEs or other
non-terminal-based applications work the way you would hope they would. We've built extensions for
a variety of popular text editors and IDEs to make them behave approximately the same way as the
Shadowenv shell integration:

* [Vim / Neovim: shadowenv.vim](https://github.com/Shopify/shadowenv.vim)
* [Emacs: shadowenv.el](https://github.com/Shopify/shadowenv.el)
* [VS Code: vscode-shadowenv](https://github.com/Shopify/vscode-shadowenv)
* [Atom: atom-shadowenv](https://github.com/Shopify/atom-shadowenv)
* [Sublime Text 3: sublime-shadowenv](https://github.com/Shopify/sublime-shadowenv)
* [IntelliJ Family: intellij-shadowenv](https://github.com/Shopify/intellij-shadowenv)

### Building Integrations

Building your own Shadowenv integration is not terribly difficult. All of our integrations—shell
hooks and editor plugins alike—just call `shadowenv hook` with some arguments and apply the result
to the process environment.

`shadowenv hook` has a few different output modes: default and `--fish` for shells; `--json` and
`--pretty-json` for languages with good JSON support; and `--porcelain` for environments where
parsing a simple binary protocol is simpler than parsing JSON.

For the most part, you're probably going to want to use `--json`. One important concept to
understand about the way we build Shadowenv integrations is that Shadowenv will instruct the calling
process to export all of the variables it sets ("exporting" a variable means that it will be
inherited by child processes: really it means that the variable is an actual environment variable,
not just a variable somewhere in the process's own memory).

Previous versions of Shadowenv (prior to 2.0.0) had a special-case for the variable
`__shadowenv_data`, which was listed as an "unexported" variable, to be set only in the process
managing the environment.

When receiving data from Shadowenv 1.3.x and earlier, you will see something like:

```
$ shadowenv hook --pretty-json ''
{
  "exported": {},
  "unexported": {
    "__shadowenv_data": "..."
  }
}
```

However, with 2.0.0 and later, you will simply see:

```
$ shadowenv hook --pretty-json ''
{
  "schema": "v2",
  "exported": {
    "__shadowenv_data": "...",
    "...": "..."
  },
  "unexported": {}
}
```

Note that we've added a "schema" field, and that schema v3 will almost certainly remove the
"unexported" element, so make sure not to depend on its presence.

### Applying variables safely

Variable names and values both come from the `.shadowenv.d` programs Shadowenv evaluates, which
can produce any text. Apply them through whatever API your language provides for *setting* an
environment variable, and never by building a string that your language then evaluates.

Concretely, in Vim script this is wrong, because the name is re-parsed as part of the command, so
one containing `|` or `"` is interpreted rather than used:

```vim
execute('let $' . name . ' = value')  " don't
```

and this is right, because the name is passed as data:

```vim
call setenv(name, value)              " do
```

The same distinction applies anywhere else: prefer `setenv`/`os.environ`/`ENV[]` over `eval`,
and prefer passing an argument vector over interpolating into a shell command.

### The `--porcelain` format

`--porcelain` emits one record per variable, terminated by `0x1E`. Each record is a list of fields
separated by `0x1F`:

```
<opcode> 0x1F <name> 0x1F <value> 0x1E
```

The opcodes are `0x01` (set, unexported — unused), `0x02` (set, exported) and `0x03` (unset, with
an empty value field). There is a trailing record separator, but don't depend on that staying true.

Shadowenv guarantees that a **name** never contains `0x1E`, `0x1F`, `=`, a newline, a carriage
return, or a NUL, and is never empty. Names that would violate this are rejected when a
`.shadowenv.d` program assigns them, and omitted from this output if one reaches it by some other
route (for example a `$__shadowenv_data` written by an older version, in which case a warning goes
to stderr). You can therefore split records and fields positionally without escaping.

That guarantee is about *framing only*. A name is still arbitrary text — `FOO BAR`, `FOO-BAR` and
`FOO | id` are all valid names that reach you — so it is not safe to interpolate one into anything
your language evaluates. See "Applying variables safely" above.

No such guarantee is made about **values**, which may contain any byte except `0x1E` and `0x1F`.

Names are deliberately *not* quoted or escaped in this format. It is a binary protocol that nothing
evaluates as a shell command, so escaping would only make the escape characters part of the name a
consumer reads back. The shell-evaluated modes (default and `--fish`) do quote both names and
values, because there the output really is evaluated by a shell.

Our suggestion moving forward into 2.0.0 and later is to treat "unexported" values read from 1.3.2
and earlier the same as "exported" values.

You can look at any or all of the editor integrations above for a roadmap to implementing your own,
but here's a minimal example in Ruby to get you started.

```ruby
require('open3')
require('json')

$shadowenv_data = nil

def on_some_event
  stdout, stderr, stat = Open3.capture3(
    'shadowenv', 'hook', '--json', $shadowenv_data,
  )
  raise(stderr) unless stat.success?

  data = JSON.parse(stdout)

  # Don't assume this will exist: it will go away in schema v3.
  data.fetch('unexported', {}).each do |name, value|
    ENV[name] = value
  end

  data['exported'].each do |name, value|
    ENV[name] = value
  end
end
```
