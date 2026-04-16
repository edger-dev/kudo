{
  description = "Kudo — composable platform for human-agent workflows";

  inputs.jig.url = "github:edger-dev/jig";

  outputs = { self, jig }:
    jig.lib.mkWorkspace
      {
        pname = "kudo";
        src = ./.;
      }
      {
        rust = { };
        docs = { beans = true; };
      };
}
