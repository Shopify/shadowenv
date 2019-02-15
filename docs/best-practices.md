---
layout: default
keywords:
comments: false

title: Best Practices
description: How to get the most out of Shadowenv

page_nav:
  prev:
    content: Shadowlisp API
    url: /shadowlisp
  next:
    content: Trust
    url: /trust

---

*This page will get more fleshed out in the future. For now, have an outline.*

## Naming files

Shadowenv files must end in `.scm`, and must be in the `.shadowenv.d` directory. We additionally
*strongly* suggest that implementors name files with a three-digit decimal number prefix to enforce
a load order, and leave plenty of space between the entries you create:

```
050_base.scm
500_node.scm
510_ruby.scm
900_user_config.scm
```

## Gitignore

We suggest `gitignore`'ing the entire `.shadowenv.d` directory. The directory should be treated as
a locally-generated artifact and be generated entirely by user tooling. At Shopify, this is done by
[`dev`](https://devproductivity.io/dev-shopifys-all-purpose-development-tool/index.html).

This is not the only valid way to go though, you may get some mileage out of committing the files.
Let us know what you do!
