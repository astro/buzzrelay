{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nix-community/naersk/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, utils, naersk }:
    let
      inherit (nixpkgs) lib;
      makeBuzzrelay = pkgs:
        let
          naersk-lib = pkgs.callPackage naersk { };
        in
        naersk-lib.buildPackage {
          pname = "buzzrelay";
          version = "${toString (self.sourceInfo.revCount or 0)}-${self.sourceInfo.shortRev or "dirty"}";
          root = ./.;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl systemd ];
          postInstall = ''
            mkdir -p $out/share/buzzrelay
            cp -r static $out/share/buzzrelay/
          '';
          checkInputs = [ pkgs.rustPackages.clippy ];
          doCheck = true;
          cargoTestCommands = x:
            x ++ [
              ''cargo clippy --all --all-features --tests -- \
                -D warnings \
                -A clippy::nonminimal_bool''
            ];
          meta.description = "The buzzing ActivityPub relay";
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
              cargo rustc rustfmt rustPackages.clippy rust-analyzer
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
