# Building and Installing netavark-plugin-pond

## Development

### Prerequisites

- **Rust toolchain** — Install via [rustup](https://rustup.rs) (`stable` channel is sufficient)
- **A C linker** — `gcc` or `clang` (pulled in transitively by `cargo`)

### Build (debug)

```shell
git clone https://github.com/caguado/netavark-plugin-pond
cd netavark-plugin-pond
cargo build
```

The debug binary is written to `target/debug/pond-netns`.

### Test suite

```shell
cargo test --all-features --workspace
```

### Lint and format

```shell
# Static analysis
cargo clippy --all-targets --all-features --workspace

# Check formatting (non-destructive)
cargo fmt --all -- --check

# Auto-format
cargo fmt --all
```

### Documentation

```shell
cargo doc --no-deps --open
```

---

## Building a release binary

```shell
cargo build --release
```

The release binary is written to `target/release/pond-netns`. It is statically
linked against all Rust dependencies; the only runtime requirements are the
system executables listed in [Runtime dependencies](#runtime-dependencies) below.

---

## Installing as a netavark plugin

netavark discovers plugins by scanning the directories listed under
`netavark_plugin_dirs` in `containers.conf(5)`. The default search paths, tried
in order, are:

```
/usr/local/libexec/netavark
/usr/libexec/netavark
/usr/local/lib/netavark
/usr/lib/netavark
```

**The binary filename becomes the driver name** used with `--driver` in
`podman network create`. The binary for this plugin is named `pond-netns`.

### Install steps

```shell
# 1. Build the release binary
cargo build --release

# 2. Copy it to the system plugin directory (requires root)
install -D -m 0755 target/release/pond-netns /usr/libexec/netavark/pond-netns
```

No changes to `containers.conf` are required when the binary is placed in one
of the default directories listed above.

To use a non-default location instead, add it to `/etc/containers/containers.conf`:

```toml
[network]
netavark_plugin_dirs = ["/opt/netavark/plugins"]
```

### Verify the plugin is discovered

```shell
podman info --format '{{range .Plugins.Network}}{{println .}}{{end}}'
```

`pond-netns` should appear in the output.

### Create a network using the plugin

```shell
podman network create \
  --driver pond-netns \
  --subnet 10.1.0.0/29 \
  --gateway 10.1.0.1 \
  --option bridge=ovsbr0 \
  --option vlan=100 \
  my-pod-network
```

See [README.md](README.md) for the full list of driver options and a pod usage example.

---

## Runtime dependencies

These executables must be present on the host at runtime. They are invoked by
the plugin during the `setup` and `teardown` lifecycle phases.

| Executable | Package (Fedora/RHEL)  | Package (Debian/Ubuntu)   | Purpose                             |
|------------|------------------------|---------------------------|-------------------------------------|
| `ovs-vsctl`| `openvswitch`          | `openvswitch-switch`      | OVS port management                 |
| `ethtool`  | `ethtool`              | `ethtool`                 | TX/SG/TSO offload configuration     |
| `nsenter`  | `util-linux`           | `util-linux`              | Entering the container netns        |

---

## File manifest (for packaging)

| Path                                   | Mode   | Description    |
|----------------------------------------|--------|----------------|
| `/usr/libexec/netavark/pond-netns`     | `0755` | Plugin binary  |

No configuration files, systemd units, shared libraries, or man pages are
installed. The binary is the sole deliverable.
