# kudo-node as a home-manager user service.
#
# A user service, not a system one: the daemon launches the user's own agent CLI
# in terminal panes, so it runs as that user, with that user's credentials.
# Start at boot needs lingering (`users.users.<name>.linger = true`).
#
# The same deployment as ./kudo-node.service, written for home-manager; the
# three load-bearing lines are asserted by tests/architecture.rs.
{ config, lib, ... }:
let
  cfg = config.services.kudo-node;
in
{
  options.services.kudo-node = {
    enable = lib.mkEnableOption "the kudo-node supervisor daemon";

    package = lib.mkOption {
      type = lib.types.package;
      description = "Package providing `kudo-node` and `moco-pty-holder`.";
    };

    hubAddr = lib.mkOption {
      type = lib.types.str;
      example = "10.100.0.1:15400";
      description = "Address of the hub this node reverse-dials.";
    };

    nodeId = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Node id; null keeps the daemon's default, the hostname.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Extra environment for the daemon (NODE_LINK, MOCO_PORT_RANGE, …).";
    };
  };

  config = lib.mkIf cfg.enable {
    # kudo-shim (worker, context, hooks) on PATH for any pane on this node.
    home.packages = [ cfg.package ];

    systemd.user.services.kudo-node = {
      # No network ordering: a user manager has no network-online.target, and
      # the daemon reverse-dials the hub forever, so it needs none.
      Unit.Description = "kudo-node — the job supervisor for this machine";
      Service = {
        ExecStart = "${cfg.package}/bin/kudo-node";
        Restart = "always";
        RestartSec = 2;

        # Signal only the daemon on stop/restart: its jobs keep running and are
        # re-adopted when it returns.
        KillMode = "process";

        Environment = [
          # Stable across restarts (%S = ~/.local/state for a user unit).
          "MOCO_DIR=%S/kudo-node"
          "KUDO_PTY_HOLDER=${cfg.package}/bin/moco-pty-holder"
          "HUB_ADDR=${cfg.hubAddr}"
        ]
        ++ lib.optional (cfg.nodeId != null) "NODE_ID=${cfg.nodeId}"
        ++ lib.mapAttrsToList (k: v: "${k}=${v}") cfg.environment;
      };
      Install.WantedBy = [ "default.target" ];
    };
  };
}
