# Script names are shared with the other repos of this stack. They avoid `test` and `fmt`,
# which lose to the shell builtin and coreutils.
# `devenv shell` gives the pinned nightly (rust-toolchain.toml), dylint and the scripts below.
# `devenv test` runs the full local check.
{ pkgs, ... }:

let
  # Not in nixpkgs. dylint builds its driver from a path baked in at build time unless
  # it is told to use crates.io; that path only exists inside the Nix sandbox.
  dylintCrate = pname: buildFeatures: hash: cargoHash: pkgs.rustPlatform.buildRustPackage {
    inherit pname buildFeatures cargoHash;
    version = "6.3.2";
    src = pkgs.fetchCrate {
      inherit pname hash;
      version = "6.3.2";
    };
    doCheck = false;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.openssl ];
  };
  cargoDylint = dylintCrate "cargo-dylint" [ "dylint/__driver_from_crates_io" ] "sha256-ylpUmv8vDyXdCaSsTfqfgN0oKEPZQQ/T0Y+CBbF8rcU=" "sha256-HWgKf7nlTZTc2HbCLuLFcTHcx/zufkeYB8V53EeQfiU=";
  dylintLink = dylintCrate "dylint-link" [ ] "sha256-3lSKsbybj/Szt3EivfwSZxa+321yt0qaZlVzX0jwYxg=" "sha256-OOe11kKOal4WGzOgK9lHkrzP+igBGSnhT/SXv61N2qA=";
  # dylint needs real rustup proxies: it strips RUSTUP_TOOLCHAIN and expects `cargo` to set
  # it again. Only dylint uses this; everything else builds with the Nix toolchain. Rustup
  # reads the same rust-toolchain.toml, so both are the same nightly.
  withRustup = ''
    export RUSTUP_HOME="$DEVENV_STATE/rustup"
    export PATH="${pkgs.rustup}/bin:$PATH"
  '';
in
{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  # zlib: the dylint driver links against it.
  packages = [ pkgs.git cargoDylint dylintLink pkgs.zlib ];

  scripts = {
    format = {
      description = "Format all Rust code";
      exec = ''cd "$DEVENV_ROOT" && cargo fmt --all'';
    };
    format-check = {
      description = "Fail when Rust code is not formatted";
      exec = ''cd "$DEVENV_ROOT" && cargo fmt --all --check'';
    };
    dylint = {
      description = "Run the slug_on_wasi_error lint on the workspace";
      exec = ''
        set -euo pipefail
        cd "$DEVENV_ROOT"
        ${withRustup}
        # The lint crate is left out: its fixtures break the lint on purpose.
        cargo dylint --all --workspace -- --all-targets --exclude otel-wasi-dylint
      '';
    };
    lint = {
      description = "Run clippy and dylint";
      exec = ''
        set -euo pipefail
        cd "$DEVENV_ROOT"
        cargo clippy --all-targets -- -D warnings
        dylint
      '';
    };
    build = {
      description = "Build the crates and the wasm examples";
      exec = ''
        set -euo pipefail
        cd "$DEVENV_ROOT"
        cargo build
        # The same command as build.command in each example's .wash/config.yaml.
        cargo build -p wasmcloud-http-hello -p otel-wasi-wasmcloud-nats-echo-example --target wasm32-wasip2 --release
      '';
    };
    tests = {
      description = "Test the crates and the lint";
      exec = ''
        set -euo pipefail
        cd "$DEVENV_ROOT"
        cargo test --locked
        ${withRustup}
        cargo test -p otel-wasi-dylint
      '';
    };
    check = {
      description = "Format check, lint, build and test everything";
      exec = ''
        set -euo pipefail
        format-check
        lint
        build
        tests
      '';
    };
  };

  enterTest = "check";
}
