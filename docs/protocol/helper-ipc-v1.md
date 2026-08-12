# Helper IPC v1

The desktop and helper exchange JSON messages over a local Unix socket on Linux
and a named pipe on Windows. Every JSON document is prefixed by a four-byte,
big-endian unsigned length. Frames larger than 1 MiB are rejected before an
allocation is made.

Every request contains:

- `protocol_version`: currently `1`
- `request_id`: UUID used for audit correlation
- `payload.command`: one command from the compile-time allowlist
- `payload.arguments`: typed arguments, when that command needs them

The first message must be `hello`. A mismatched protocol ends the connection.
Linux additionally checks `SO_PEERCRED`; Windows uses a pipe ACL and verifies the
client token. A successful transport connection is not treated as authorization.

The helper never accepts an executable path, shell command, URL, arbitrary file
path, service name, or PID. A runtime generation is referenced only by UUID and a
64-character lowercase SHA-256. The helper derives all paths beneath its fixed
runtime directory and rechecks ownership, regular-file status, and hash.

Requests have a five-second I/O timeout. Process start/stop have bounded service-
side timeouts and cancellation is represented by a subsequent stop/cleanup
request. Audit logs record the command name, peer identity, request ID, result,
and duration; secrets and raw config are never recorded.
