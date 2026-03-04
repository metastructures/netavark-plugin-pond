## 1. Fix route-before-up ordering in plumbing.rs

- [x] 1.1 In `src/net_ns/plumbing.rs`, move `container.netlink.set_up(LinkID::ID(inner.header.index))` to immediately after `add_addr` and before `add_route`
- [x] 1.2 Add an inline comment at the `set_up` call explaining the ordering constraint: inner interface must be UP before `add_route` so the kernel considers the connected route active
- [x] 1.3 Run `cargo test --all-features --workspace` and confirm all unit tests pass
- [x] 1.4 Run `cargo clippy --all-targets --all-features --workspace` and confirm no new warnings

## 2. Verify fix on live system

- [ ] 2.1 Build the release binary: `cargo build --release`
- [ ] 2.2 Install to plugin path: `install -m 0755 target/release/pond-netns /usr/libexec/netavark/pond-netns`
- [ ] 2.3 Recreate the test network: `podman network rm <name>` then `podman network create --driver pond-netns --subnet ... --gateway ... --opt bridge=... --opt vlan=...`
- [ ] 2.4 Run Test A (dynamic IP, no userns): `podman pod create --network <name> --name test-a && podman pod start test-a` — confirm exit 0
- [ ] 2.5 Run Test B (static IP, no userns): `podman pod create --network <name>:ip=<addr> --name test-b && podman pod start test-b` — confirm exit 0
- [ ] 2.6 Confirm veth exists in host netns and OVS port is registered: `ip link show <upstream>` and `ovs-vsctl list-ports <bridge>`
- [ ] 2.7 Clean up test pods: `podman pod rm -f test-a test-b`

## 3. Write DEPLOYMENT.md

- [x] 3.1 Create `DEPLOYMENT.md` at the project root with sections: Prerequisites, Quadlet File Structure, Systemd Service Dependencies, Starting and Stopping, Manual Plugin Testing, Troubleshooting
- [x] 3.2 Document the three quadlet files (`.network`, `.pod`, `.container`)
- [x] 3.3 Document the systemd dependency chain (`Requires=` + `After=` from network service to pod service to container service)
- [x] 3.4 Document manual plugin testing: `pond-netns info`, `pond-netns create` with piped JSON, `pond-netns setup <netns-path>` with piped JSON, `pond-netns teardown`; reference example payloads in `examples/`
- [x] 3.5 Document troubleshooting: missing `Gateway=` symptom and fix; stale IPAM state recovery (`podman network rm` + restart); verifying plugin discovery (`podman info`)

## 4. Update changelog and version

- [ ] 4.1 Add an entry to `CHANGELOG.md` under a new patch version: describe the route-ordering fix and the new `DEPLOYMENT.md`
- [x] 4.2 Bump the patch version in `Cargo.toml` (e.g. `0.1.0` → `0.1.1`)
- [x] 4.3 Run `cargo build` to confirm `Cargo.lock` is updated
