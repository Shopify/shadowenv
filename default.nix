{ pkgs ? (import <nixpkgs> { }) }:

with pkgs;
rustPlatform.buildRustPackage rec {
  pname = "shadowenv";
  version = lib.removeSuffix ''"'' (lib.removePrefix ''version = "''
    (lib.findFirst (line: lib.hasPrefix ''version = "'' line) ''version = ""''
      (lib.splitString "\n" (builtins.readFile (./. + "/Cargo.toml")))));
  src = builtins.fetchGit { url = "file://${builtins.toString ./.}"; };
  cargoSha256 = "1ksrqyb88z0sav0hg2asly5my7zgqvjla22a47wb0sppj9f0z70b";
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

  meta = with lib; {
    homepage = "https://shopify.github.io/shadowenv/";
    description =
      "reversible directory-local environment variable manipulations";
    license = licenses.mit;
  };
}
