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
