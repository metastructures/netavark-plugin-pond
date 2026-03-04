## Context

`plumbing::provision()` implements the `setup` lifecycle hook. It builds the veth pair, assigns an IP to the container-side interface, installs a default route, and then brings both interfaces up. The final step — `set_up()` on the container-side interface — must occur before `add_route()`, not after, because Linux only considers an interface's connected routes active when the interface is UP.

The bug was found during live system testing (podman 5.4 / Fedora 42 / kernel 6.18) using podman quadlets. All three test variants (dynamic IP, static IP, static IP + userns) failed with:

```
netavark: plugin "pond-netns" failed: exit code 1,
message: add default route via <gw>: Netlink error: Network is unreachable (os error 101)
```

The existing unit tests in `plumbing.rs` mock the netlink operations and cover only `extract_mac` and `build_ovs_add_port_args`. Neither exercises the kernel routing stack, so the ordering bug was never caught by CI.

## Goals / Non-Goals

**Goals:**
- Correct the `set_up` / `add_route` ordering so every pod start succeeds.
- Add `DEPLOYMENT.md` covering quadlet configuration, manual testing, and operational troubleshooting, based on findings from the live test session.
- Document the test coverage gap so future contributors understand why a real-netns integration test is needed.

**Non-Goals:**
- Adding kernel-level integration tests (requires a real network namespace; out of scope for this change but noted as a follow-up).
- Fixing the `--userns auto:... + --ip` BoltDB IPAM interaction in podman 5.4 (upstream podman issue; the pod starts correctly once the route bug is resolved).
- Changing any public API, option, or configuration format.

## Decisions

### Move `set_up(inner)` to before `add_route`

**Decision:** Reorder operations in `provision()` so the container-side interface is brought UP immediately after `add_addr`, before `add_route`.

**Rationale:** Linux route installation requires the gateway to be reachable via an active connected route. `add_addr` creates the connected route entry, but the entry is only active when the interface is UP. The original scripts in the README do `ip link set <iface> up` before `ip route add`, which is the correct kernel-mandated order. The plugin must match this.

**Alternatives considered:**
- *Add `NLM_F_REPLACE` or route flags to bypass reachability check* — not possible; `ENETUNREACH` is a hard kernel check, not a flag-controlled behaviour.
- *Bring both ends up before configuring addresses* — valid, but unnecessarily moves the host-side `set_up` earlier, adding no benefit while changing more code than necessary.

### Re-read inner link after `set_up` (keep existing `get_link` for MAC)

**Decision:** Keep the second `get_link` call (to read the MAC address) after `set_up`, since the MAC is stable once the interface exists and the second read is already there for that purpose.

**Rationale:** No behaviour change needed here; the second read is cheap and the position is already correct.

### `DEPLOYMENT.md` scope

**Decision:** Cover quadlet file authoring, the systemd service dependency graph, manual plugin invocation with JSON payloads, and common failure modes discovered during the live session (missing `Gateway=`, IPAM state cleanup).

**Rationale:** The live test session produced concrete, validated procedures that are not captured anywhere in the existing docs. `INSTALL.md` covers build and install; `DEPLOYMENT.md` covers operational use.

## Risks / Trade-offs

**[Risk] MAC read after `set_up` is still a separate `get_link` call** → The MAC is assigned at veth creation and does not change; the call is safe. No mitigation needed.

**[Risk] No regression test for route ordering** → Any future refactor of `provision()` could reintroduce the bug silently. Mitigation: add a comment in the code marking the ordering constraint and reference this change in the test gap note.

**[Risk] BoltDB + `--userns` + `--ip` IPAM issue (Test C)** → With podman 5.4 + BoltDB, specifying a static IP via `--network <net>:ip=<addr>` combined with `--userns auto:...` triggers an IPAM warning. The pod still starts once the route bug is fixed (netavark falls back to dynamic allocation), but the intended static IP may not be honoured. Mitigation: document in `DEPLOYMENT.md`; track as a separate upstream issue.

## Migration Plan

1. Apply the one-line reorder in `plumbing.rs`.
2. Run `cargo test` to confirm no unit test regressions.
3. Verify on live system: `podman pod create --network <pond-net> && podman pod start <pod>` succeeds.
4. Merge and release as a patch version bump.

Rollback: revert the single commit; no state migration required.

## Open Questions

- Should a `#[cfg(test)]` integration test using `unshare` / `ip netns` be added in this change, or tracked as a separate follow-up? Current consensus: separate follow-up to keep this change minimal.
