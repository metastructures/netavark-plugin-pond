## Why

`plumbing::provision()` calls `add_route()` before `set_up()` on the container-side veth interface. Linux only considers an interface's connected routes active when the interface is UP, so the default route via the configured gateway always fails with `ENETUNREACH` (errno 101), making every pod start fail. Discovered during live system testing with podman quadlets.

## What Changes

- Move `set_up(inner)` in `plumbing::provision()` to execute after `add_addr()` and before `add_route()`.
- Add a `DEPLOYMENT.md` documenting quadlet-based deployment, manual plugin testing, and troubleshooting procedures.

## Capabilities

### New Capabilities

- `deployment-workflow`: End-to-end documentation for deploying pond-netns via podman quadlets and systemd, including manual testing of the plugin binary and operational troubleshooting.

### Modified Capabilities

- `network-lifecycle`: The veth provisioning sequence within `setup` changes — interface is brought UP before the default route is installed. The observable contract (pod gets a configured veth connected to OVS) is unchanged; this corrects the implementation to match the specified behaviour.

## Impact

- `src/net_ns/plumbing.rs` — operation reorder in `provision()`; no API or type changes.
- `DEPLOYMENT.md` — new file.
- Tests in `plumbing.rs` do not cover kernel routing behaviour; a note is added to the test gap rationale in `design.md`.
