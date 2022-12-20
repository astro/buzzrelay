{ self }:
{ config, lib, pkgs, ... }: {
  options.services.buzzrelay = with lib; {
    enable = mkEnableOption "Enable Fedi.buzz relay";
    listenPort = mkOption {
      type = types.int;
      default = 8000;
    };
    hostName = mkOption {
      type = types.str;
    };
    privKeyFile = mkOption {
      type = types.str;
    };
    pubKeyFile = mkOption {
      type = types.str;
    };
    database = mkOption {
      type = types.str;
      default = "buzzrelay";
    };
    user = mkOption {
      type = types.str;
      default = "relay";
    };
    group = mkOption {
      type = types.str;
      default = "relay";
    };
  };

  config =
    let
      cfg = config.services.buzzrelay;
      configFile = builtins.toFile "buzzrelay.toml" (
        lib.generators.toYAML {} {
          hostname = cfg.hostName;
          listen_port = cfg.listenPort;
          priv_key_file = cfg.privKeyFile;
          pub_key_file = cfg.pubKeyFile;
          db = "postgres://${cfg.user}@localhost/${cfg.database}";
        });
    in
      lib.mkIf cfg.enable {
        users.users.${cfg.user} = {
          inherit (cfg) group;
          isSystemUser = true;
        };
        users.groups.${cfg.group} = {};

        services.postgresql = {
          enable = true;
          ensureDatabases = [ cfg.database ];
          ensureUsers = [ {
            name = cfg.user;
            ensurePermissions = {
              "DATABASE ${cfg.database}" = "ALL PRIVILEGES";
            };
          } ];
        };

        systemd.services.buzzrelay = {
          wantedBy = [ "multi-user.target" ];
          after = [ "network-online.target" ];
          serviceConfig = {
            Type = "notify";
            ExecStart = "${self.packages.${pkgs.system}.buzzrelay}/bin/buzzrelay ${lib.escapeShellArg configFile}";
            User = cfg.user;
            Group = cfg.group;
            ProtectSystem = "full";
            Restart = "always";
            RestartSec = "10s";
            WatchdogSec = "600s";
          };
        };
      };
}
