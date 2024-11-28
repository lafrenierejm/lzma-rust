{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
    git-hooks.url = "github:cachix/git-hooks.nix";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      advisory-db,
      git-hooks,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (
          p:
          # Use latest stable release of Rust.
          p.rust-bin.stable.latest.default
        );
        src = craneLib.cleanCargoSource ./.;

        nativeBuildInputs =
          with pkgs;
          lib.lists.flatten [
            (lib.optionals (system == "x86_64-linux") [ cargo-tarpaulin ])
            (lib.optionals stdenv.isDarwin [
              # Additional darwin specific inputs can be set here
              gcc
              libiconv
            ])
          ];

        # Build just the Cargo dependencies to maximize dependencies.
        cargoArtifacts = craneLib.buildDepsOnly { inherit src nativeBuildInputs; };
      in
      rec {
        # `nix flake check`
        checks =
          {
            audit = craneLib.cargoAudit { inherit src advisory-db; };

            clippy = craneLib.cargoClippy {
              inherit cargoArtifacts src nativeBuildInputs;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            };

            doc = craneLib.cargoDoc { inherit cargoArtifacts src; };

            fmt = craneLib.cargoFmt { inherit src; };

            test = craneLib.cargoTest {
              inherit cargoArtifacts src nativeBuildInputs;
            };

            git-hooks = git-hooks.lib."${system}".run {
              src = ./.;
              hooks = {
                editorconfig-checker.enable = true;
                nixfmt-rfc-style.enable = true;
                rustfmt.enable = true;
                typos.enable = true;
              };
            };
          }
          // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
            # Check code coverage (note: this will not upload coverage anywhere)
            coverage = craneLib.cargoTarpaulin { inherit cargoArtifacts src; };
          };

        # `nix develop`
        devShells.default = craneLib.devShell {
          inherit (self.checks.${system}.git-hooks) shellHook;
          inputsFrom = builtins.attrValues self.checks;
        };
      }
    );
}
