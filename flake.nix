{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils }:
    let
      makeBuzzrelay = pkgs:
        pkgs.rustPlatform.buildRustPackage rec {
          pname = "buzzrelay";
          version = (
            pkgs.lib.importTOML ./Cargo.toml
          ).package.version + "-${toString (self.sourceInfo.revCount or 0)}-${self.sourceInfo.shortRev or "dirty"}";
          src = pkgs.runCommand "${pname}-${version}-src" {
            preferLocalBuild = true;
          } ''
            mkdir $out
            cd ${self}
            cp -ar Cargo.{toml,lock} static src $out/
          '';
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config rustPackages.clippy ];
          buildInputs = with pkgs; [ openssl systemd ];
          postInstall = ''
            mkdir -p $out/share/buzzrelay
            cp -r static $out/share/buzzrelay/
          '';
          postCheck = ''
            cargo clippy --all --all-features --tests -- \
              -D warnings
          '';
          meta = {
            description = "The buzzing ActivityPub relay";
            mainProgram = "buzzrelay";
          };
        };
    in
    utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          packages = {
            default = self.packages."${system}".buzzrelay;
            buzzrelay = makeBuzzrelay pkgs;
          };

          apps.default = utils.lib.mkApp {
            drv = self.packages."${system}".default;
          };

          devShells.default = with pkgs; mkShell {
            nativeBuildInputs = [
              pkg-config
              openssl systemd
              cargo rustc rustfmt rustPackages.clippy rust-analyzer cargo-outdated
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
          };
        })
    // {
      overlays.default = (_: prev: {
        buzzrelay = makeBuzzrelay prev;
      });

      nixosModules.default = import ./nixos-module.nix { inherit self; };
    };
}
