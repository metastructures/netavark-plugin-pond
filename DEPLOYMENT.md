# Deploying netavark-plugin-pond with Podman Quadlets

This guide covers deploying a pod that uses the `pond-netns` driver via
[podman quadlet](https://docs.podman.io/en/latest/markdown/podman-systemd.unit.5.html)
files and systemd.  It also explains how to invoke the plugin binary directly
for manual testing and diagnostics.

---

## Prerequisites

Before any quadlet will start successfully:

1. **Plugin installed** — `pond-netns` binary must be in a netavark plugin
   directory (default: `/usr/libexec/netavark/`).  See [INSTALL.md](INSTALL.md).

2. **OVS bridge exists** — The bridge named in `Options=bridge=` must already
   exist and be managed by a separate service (e.g. `openvswitch.service`).
   The plugin does not create OVS bridges.

3. **Runtime dependencies on PATH** — `ovs-vsctl`, `nsenter`, and `ethtool`
   must be installed.  See [INSTALL.md](INSTALL.md#runtime-dependencies).

4. **Verify plugin discovery:**
   ```bash
   podman info --format '{{range .Plugins.Network}}{{println .}}{{end}}'
   # pond-netns must appear in the output
   ```

---

## Quadlet File Structure

A deployment requires three quadlet files placed in `/etc/containers/systemd/`:

```
/etc/containers/systemd/
├── <name>.network      ← creates the podman network (pond-netns driver)
├── <name>.pod          ← creates and owns the pod
└── <sidecar>.container ← one file per container in the pod
```

systemd-generator reads these files at boot (or after `systemctl daemon-reload`)
and generates the corresponding `.service` units automatically.

### `<name>.network` — the network

```ini
[Unit]
Description=My service network
After=openvswitch.service
Requires=openvswitch.service        # OVS bridge must be up first

[Network]
NetworkName=my-service              # podman network name
Driver=pond-netns
Subnet=10.1.0.0/29
Gateway=10.1.0.1                    # REQUIRED — plugin uses this for the default route
Internal=true                       # plugin enforces this; safe to include explicitly
DisableDNS=true                     # plugin enforces this; safe to include explicitly
Options=bridge=ovsbr0               # REQUIRED — pre-existing OVS bridge name
Options=vlan=100                    # REQUIRED — 802.1Q VLAN ID (1–4094)
Options=upstream=myapp0             # optional — host-side veth name (≤15 chars)
                                    #   if omitted: derived as pond<crc32hex(NetworkName)>
Options=mtu=1500                    # optional — default 1500
Options=min_port=1024               # optional — net.ipv4.ip_unprivileged_port_start

[Install]
WantedBy=default.target
```

> **`Gateway=` is required.** The plugin uses this value when installing the
> default route inside the container network namespace.  Omitting it causes
> `setup` to fail with *"no gateway configured in subnet"*.

### `<name>.pod` — the pod

```ini
[Unit]
Description=My service pod
Requires=<name>-network.service     # generated service name: <NetworkName>-network.service
After=<name>-network.service

[Pod]
PodName=my-service
Network=my-service.network:ip=10.1.0.2   # inline :ip= for a static address
                                          # omit :ip= for dynamic IPAM allocation

[Install]
WantedBy=default.target
```

Notes:
- The `Network=` value uses the quadlet filename (`.network` extension), not
  the `NetworkName`.
- Prefer the inline `Network=<name>.network:ip=<addr>` form over separate
  `Network=` + `IP=` directives to avoid ambiguity.
- When using `UserNS=auto:...` with a static IP in podman 5.4 + BoltDB IPAM,
  an `IPAM error` warning may appear in the journal.  This is a known podman
  limitation; the pod starts correctly once the network is healthy.

### `<sidecar>.container` — a container in the pod

```ini
[Unit]
Description=My service sidecar
Requires=<name>-pod.service         # generated name: <PodName>-pod.service
After=<name>-pod.service

[Container]
Pod=<name>.pod                      # quadlet filename, not PodName
ContainerName=my-sidecar
Image=registry.example.com/my-image:latest
# ...volumes, env, capabilities...

[Install]
WantedBy=default.target
```

> **Do not set `Network=` on a container that uses `Pod=`.**  The container
> inherits the pod's network namespace automatically.  A leftover
> `Network=ns:/run/netns/...` from a pre-plugin scripted setup must be removed.

---

## Systemd Service Dependency Chain

The quadlet generator produces one systemd service per file.  The resulting
dependency graph is:

```
openvswitch.service
    │
    └── <NetworkName>-network.service   (from .network quadlet)
            │
            └── <PodName>-pod.service   (from .pod quadlet)
                    │
                    ├── <sidecar1>.service   (from .container quadlets)
                    └── <sidecar2>.service
```

The `.network` service runs `podman network create` on start and
`podman network rm` on stop.  The `.pod` service runs
`podman pod create --replace` followed by `podman pod start`.

Verify the generated units after placing quadlet files:

```bash
systemctl daemon-reload
systemctl cat <NetworkName>-network.service
systemctl cat <PodName>-pod.service
```

---

## Starting and Stopping

```bash
# Start everything (dependencies resolve automatically)
sudo systemctl start <PodName>-pod.service

# Stop the pod (leaves the network running)
sudo systemctl stop <PodName>-pod.service

# Stop everything including the network
sudo systemctl stop <PodName>-pod.service <NetworkName>-network.service

# Enable at boot
sudo systemctl enable <NetworkName>-network.service
sudo systemctl enable <PodName>-pod.service
sudo systemctl enable <sidecar>.service
```

---

## Manual Plugin Testing

The `pond-netns` binary speaks the netavark plugin protocol: JSON on stdin,
JSON (or nothing) on stdout, errors on stderr.  You can invoke it directly
without podman for diagnostics or development.

Example JSON payloads are in [`examples/`](examples/).

### Print plugin version and API info

```bash
/usr/libexec/netavark/pond-netns info
```

### Test the `create` phase (no kernel resources created)

```bash
cat examples/create.json | /usr/libexec/netavark/pond-netns create
```

Expected: normalized `Network` JSON printed to stdout, exit 0.

You can also pipe inline JSON to quickly validate option parsing:

```bash
echo '{
  "name": "test-net",
  "id": "aabbcc",
  "driver": "pond-netns",
  "subnets": [{"subnet": "10.99.0.0/29", "gateway": "10.99.0.1"}],
  "options": {"bridge": "ovsbr0", "vlan": "100"}
}' | /usr/libexec/netavark/pond-netns create
```

### Test the `setup` phase (creates kernel resources)

`setup` requires a real network namespace and a running OVS bridge.

```bash
# 1. Create a throw-away namespace
sudo ip netns add pond-test

# 2. Run setup — use examples/setup.json or craft your own
sudo cat examples/setup.json | \
  /usr/libexec/netavark/pond-netns setup /run/netns/pond-test

# 3. Inspect results
ip link show               # upstream veth (e.g. pond4a3f1234) should appear
sudo ip netns exec pond-test ip addr show   # inner veth with IP
sudo ovs-vsctl list-ports ovsbr0           # upstream should be listed
```

Expected: `StatusBlock` JSON printed to stdout, exit 0.

### Test the `teardown` phase

```bash
sudo cat examples/teardown.json | \
  /usr/libexec/netavark/pond-netns teardown /run/netns/pond-test

# Verify cleanup
ip link show               # upstream veth should be gone
sudo ovs-vsctl list-ports ovsbr0   # port should be removed

# Remove the namespace
sudo ip netns del pond-test
```

Expected: no output, exit 0.

### End-to-end test with podman (no systemd)

This tests the full IPAM + plugin path without quadlets:

```bash
# Create the network
sudo podman network create \
  --driver pond-netns \
  --subnet 10.1.0.0/29 \
  --gateway 10.1.0.1 \
  --opt bridge=ovsbr0 \
  --opt vlan=100 \
  --opt upstream=test0 \
  test-pond-net

# Verify network config
sudo podman network inspect test-pond-net

# Test A — dynamic IP
sudo podman pod create --network test-pond-net --name test-a
sudo podman pod start test-a
sudo podman pod inspect test-a
sudo podman pod rm -f test-a

# Test B — static IP
sudo podman pod create --network test-pond-net:ip=10.1.0.2 --name test-b
sudo podman pod start test-b
sudo podman pod rm -f test-b

# Clean up
sudo podman network rm test-pond-net
```

---

## Troubleshooting

### Plugin not discovered

**Symptom:** `podman network create --driver pond-netns` fails with
*"unknown network driver"* or pond-netns is absent from `podman info`.

**Fix:**
```bash
# Check the binary is in a plugin directory
ls -la /usr/libexec/netavark/pond-netns

# Confirm netavark finds it
podman info --format '{{range .Plugins.Network}}{{println .}}{{end}}'
```

See [INSTALL.md](INSTALL.md) for the full list of plugin search paths and how
to add a custom directory via `containers.conf`.

---

### `no gateway configured in subnet` / setup always fails

**Symptom:** Pod start fails with:
```
netavark: plugin "pond-netns" failed: exit code 1,
message: no gateway configured in subnet
```

**Cause:** The `.network` quadlet is missing a `Gateway=` line.

**Fix:** Add `Gateway=<first-usable-ip>` to the `[Network]` section, then
recreate the network:

```bash
sudo podman network rm <NetworkName>
sudo systemctl restart <NetworkName>-network.service
sudo podman network inspect <NetworkName>   # verify gateway appears
```

---

### `Network is unreachable` on route installation

**Symptom:** Pod start fails with:
```
netavark: plugin "pond-netns" failed: exit code 1,
message: add default route via <gw>: Netlink error: Network is unreachable (os error 101)
```

**Cause:** Plugin version ≤ 0.1.0 had a bug where `set_up` was called after
`add_route`, causing `ENETUNREACH`.  Fixed in 0.1.1.

**Fix:** Upgrade the plugin binary to ≥ 0.1.1.

---

### IPAM error on pod start

**Symptom:** Journal shows:
```
IPAM error: failed to get ips for container ID <id> on network <name>
```

**Cause:** Stale IPAM state from a previously incomplete pod teardown, or
the network was removed and re-created but the IPAM database still holds a
reservation from the old network ID.

**Fix:**
```bash
# Stop any running pod
sudo systemctl stop <PodName>-pod.service

# Remove the network (clears IPAM state)
sudo podman network rm <NetworkName>

# Restart the network service to recreate it
sudo systemctl restart <NetworkName>-network.service

# Verify the network is clean (containers should be empty)
sudo podman network inspect <NetworkName>

# Retry the pod
sudo systemctl start <PodName>-pod.service
```

If stale IPAM entries persist after `podman network rm`, the IPAM database can
be reset (it is recreated automatically):

```bash
sudo systemctl stop <PodName>-pod.service
sudo podman network rm <NetworkName>
sudo rm -f /run/containers/networks/ipam.db   # runtime file, safe to delete
sudo systemctl start <NetworkName>-network.service
```

---

### OVS `del-port` warning during teardown

**Symptom:**
```
pond-netns: ovs-vsctl del-port warning (may already be gone): no port named <upstream>
```

**Cause:** Teardown is called as cleanup after a failed setup; the port was
never added.  This is expected and non-fatal — the plugin exits 0.

---

### Container fails to start after pod is running

**Symptom:** The pod starts but a sidecar container fails with a network error.

**Check:** Ensure the `.container` quadlet has `Pod=<name>.pod` and does **not**
have a conflicting `Network=` directive (e.g. a leftover `Network=ns:/run/netns/...`
from a pre-plugin scripted setup).  Remove any `Network=` line from containers
that use `Pod=`.
