{ pkgs ? (import <nixpkgs> {}) }:

with pkgs;
rustPlatform.buildRustPackage rec {
  name = "shadowenv";
  src = ./.;
  cargoSha256 = "0v6sazdykdm7jsclf8mswr3s7rrlxm6f1kqpk4ki1yy2rr4hwqsr";
  buildInputs = lib.optionals stdenv.isDarwin [ darwin.apple_sdk.frameworks.Security ];
}
