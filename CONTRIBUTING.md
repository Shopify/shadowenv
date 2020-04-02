# Releasing Shadowenv

1. Be running macOS
1. Bump the version in `Cargo.toml`
1. Build a release with `cargo build --release`
1. Manually verify that the release at `target/release/shadowenv` seems ok, at least prints the
   expected version with `--version`
1. Commit the changed `Cargo.toml` and `Cargo.lock` with the message "bump version to x.y.z".
1. Tag the commit as x.y.z (no leading 'v')
1. Push the commit and the tag
1. `cp target/release/shadowenv shadowenv-x86_64-apple-darwin`
1. Open https://github.com/Shopify/shadowenv/releases and add `shadowenv-x86_64-apple-darwin` as
   a file to the tag you just pushed.

## (shopify-internal) releasing to `dev`:

1. `dev clone dev`, fetch master, etc.
1. `dev bump-shopify-repo shadowenv <new-sha>`
1. `dev test-nix-package shadowenv`
1. Commit, put, PR, you know the drill.
