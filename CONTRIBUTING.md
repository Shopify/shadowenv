# Releasing Shadowenv

1. Be running macOS
1. Bump the version in `Cargo.toml`
1. Build a release with `cargo build --release`
1. Manually verify that the release at `target/release/shadowenv` seems ok, at least prints the
   expected version with `--version`
1. In `default.nix` replace the `cargoSha256` value with "0000000000000000000000000000000000000000000000000000"
1. Run `nix build` and replace `cargoSha256` with the expected hash (which might not have changed after all)
1. Commit with the message "bump version to x.y.z".
1. Tag the commit as x.y.z (no leading 'v')
1. Push the commit and the tag
1. `cp target/release/shadowenv shadowenv-x86_64-apple-darwin`
1. Open https://github.com/Shopify/shadowenv/releases and add `shadowenv-x86_64-apple-darwin` as
   a file to the tag you just pushed.

## (shopify-internal) releasing to `dev`:

1. `dev clone dev`, fetch master, etc.
1. Run `dev prefetch-shopify-repo shadowenv <version>` to get nix hash
1. In the  `shadowenv` section of `src/nixpkgs-overlay/default.nix`
   1. Update `version`
   1. Update `src > sha256` with the nix hash from step #2 above
   1. Update `cargoDeps > outputHash` with the `cargoHash256` value from Shadowenv's `default.nix` if needed
1. `dev test-nix-package shadowenv`
1. Commit, put, PR, you know the drill.
