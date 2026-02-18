{ pkgs, lib, config, ... }:

{
  # https://devenv.sh/packages/
  packages = with pkgs; [
    git
    bun
    pkg-config
    rustup
  ];

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer"];
  };

  # https://devenv.sh/git-hooks/
  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  # https://devenv.sh/basics/
  enterShell = ''
    # Set up rustup default toolchain (needed for cargo fmt/clippy subcommands)
    rustup default stable

    echo "Welcome to KrakenTUI dev environment"
    git --version
  '';
}
