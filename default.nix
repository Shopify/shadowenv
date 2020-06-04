{ pkgs ? (import <nixpkgs> { }) }:

with pkgs;
rustPlatform.buildRustPackage rec {
  pname = "shadowenv";
  version = lib.removeSuffix ''"'' (lib.removePrefix ''version = "''
    (lib.findFirst (line: lib.hasPrefix ''version = "'' line) ''version = ""''
      (lib.splitString "\n" (builtins.readFile (./. + "/Cargo.toml")))));
  src = builtins.fetchGit { url = "file://${builtins.toString ./.}"; };
  cargoSha256 = "1bjkwn57vm3in8lajhm7p9fjwyqhmkrb3fyq1k7lqjvrrh9jysb2";
  nativeBuildInputs = [ installShellFiles ];
  buildInputs =
    lib.optionals stdenv.isDarwin [ darwin.apple_sdk.frameworks.Security ];

  postInstall = ''
    installManPage man/man1/shadowenv.1
    installManPage man/man5/shadowlisp.5
    installShellCompletion --bash sh/completions/shadowenv.bash
    installShellCompletion --fish sh/completions/shadowenv.fish
    installShellCompletion --zsh sh/completions/_shadowenv
  '';

  meta = with stdenv.lib; {
    homepage = "https://shopify.github.io/shadowenv/";
    description =
      "reversible directory-local environment variable manipulations";
    license = licenses.mit;
  };
}
