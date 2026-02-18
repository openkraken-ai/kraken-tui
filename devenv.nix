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
  # Custom entries because Cargo.toml lives in native/, not the repo root.
  git-hooks.hooks = {
    clippy = {
      enable = true;
      entry = "cargo clippy --manifest-path native/Cargo.toml -- -D warnings";
      files = "\\.rs$";
      pass_filenames = false;
    };
    rustfmt = {
      enable = true;
      entry = "cargo fmt --manifest-path native/Cargo.toml --check";
      files = "\\.rs$";
      pass_filenames = false;
    };
  };

  # https://devenv.sh/basics/
  enterShell = ''
    # Set up rustup default toolchain (needed for cargo fmt/clippy subcommands)
    rustup default stable

    echo "Welcome to KrakenTUI dev environment"
    git --version
  '';
}
