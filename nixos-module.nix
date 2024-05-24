{ self }:
{ config, lib, pkgs, ... }: {
  options.services.buzzrelay = with lib; {
    enable = mkEnableOption "Enable Fedi.buzz relay";
    streams = mkOption {
      type = with types; listOf str;
      default = [
        "https://fedi.buzz/api/v1/streaming/public"
      ];
    };
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
      default = "buzzrelay";
    };
    group = mkOption {
      type = types.str;
      default = "buzzrelay";
    };
    logLevel = mkOption {
      type = types.enum [ "ERROR" "WARN" "INFO" "DEBUG" "TRACE" ];
      default = "INFO";
    };

    redis = {
      connection = mkOption {
        type = with types; nullOr str;
        default = null;
      };
      passwordFile = mkOption {
        type = with types; nullOr path;
        default = null;
      };
      inTopic = mkOption {
        type = types.str;
        default = "relay-in";
      };
    };
  };

  config =
    let
      cfg = config.services.buzzrelay;
      configFile = builtins.toFile "buzzrelay.toml" (
        lib.generators.toYAML {} {
          streams = cfg.streams;
          hostname = cfg.hostName;
          listen_port = cfg.listenPort;
          priv_key_file = cfg.privKeyFile;
          pub_key_file = cfg.pubKeyFile;
          db = "host=/var/run/postgresql user=${cfg.user} dbname=${cfg.database}";
          redis = if cfg.redis.connection != null
                  then {
                    connection = cfg.redis.connection;
                    password_file = cfg.redis.passwordFile;
                    in_topic = cfg.redis.inTopic;
                  }
                  else null;
        });
      inherit (self.packages.${pkgs.system}) buzzrelay;
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
            ensureDBOwnership = true;
          } ];
        };

        systemd.services.buzzrelay = {
          wantedBy = [ "multi-user.target" ];
          after = [ "postgresql.service" "network-online.target" ];
          wants = [ "network-online.target" ];
          environment.RUST_LOG = "buzzrelay=${cfg.logLevel}";
          serviceConfig = {
            Type = "notify";
            WorkingDirectory = "${buzzrelay}/share/buzzrelay";
            ExecStart = "${buzzrelay}/bin/buzzrelay ${lib.escapeShellArg configFile}";
            User = cfg.user;
            Group = cfg.group;
            ProtectSystem = "full";
            Restart = "always";
            RestartSec = "1s";
            WatchdogSec = "1800s";
            LimitNOFile = 40000;
          };
        };
      };
}
