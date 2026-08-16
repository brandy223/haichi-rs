# haichi

Declarative GNOME screen layout from a TOML file, applied via `gdctl`.

Instead of a hand-written `gdbus call … ApplyMonitorsConfig` invocation, you
describe only the variables you care about — position, scale, transform, mode,
which screen is primary — and the tool resolves each declared screen to whatever
connector it currently sits on.

## Building

```sh
cargo build --release
install -Dm755 target/release/haichi ~/.local/bin/haichi   # or wherever's on your PATH
```

Requires a Rust toolchain (edition 2024). State is read straight off the
session bus via [`zbus`](https://docs.rs/zbus), which is pure Rust and needs
no `libdbus`; `gdctl` is only shelled out to by `apply`, to actually change
the configuration.

## Usage

```sh
haichi export -o layout.toml   # capture the live layout
haichi apply layout.toml -n    # print the gdctl command, run nothing
haichi apply layout.toml -V    # let Mutter validate it, apply nothing
haichi apply layout.toml       # apply, persistently
```

Requires `gdctl` (ships with Mutter since GNOME 48) on `$PATH` at runtime.

`apply`'s `config` argument is optional. Left out, it defaults to
`$XDG_CONFIG_HOME/haichi/config.toml`, falling back to
`~/.config/haichi/config.toml` when `XDG_CONFIG_HOME` is unset — so once your
layout lives there, `haichi apply` with no arguments is what you put in a
login or hotplug hook. An explicit path always overrides the default.

### `apply` flags

| Flag | Effect |
| --- | --- |
| `-n`, `--dry-run` | Print the `gdctl` command and exit. Nothing is executed. |
| `-V`, `--verify` | Hand the layout to Mutter for validation without applying it. Catches geometry errors (`Logical monitors not adjacent`) that this tool does not model. |
| `--no-persistent` | Apply without writing `monitors.xml`. See below — you almost never want this. |

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Applied, verified, or **skipped because a declared screen is not plugged in** |
| 1 | `gdctl` or D-Bus failed |
| 2 | The TOML does not describe an applicable layout |

A layout naming hardware that is not connected is not an error — it just is not
the layout for the machine's current state, so `apply` reports what is missing
and exits 0. That makes `haichi apply` (relying on the default config path
above) safe to run unconditionally at login or from a hotplug hook.

## Schema

```toml
layout-mode = "logical"        # optional: "logical" | "physical"

[screens.p2710s]
vendor = "LHC"                 # required, from D-Bus (raw PNP code)
product = "P2710S"             # required
serial = "0000000000000"       # required
mode = "2560x1440@240.002"     # required, exact mode id
x = 0                          # default 0
y = 0                          # default 0
scale = 1                      # default 1
transform = "270"              # default "normal"
primary = true                 # exactly one screen must set this
connector = "DP-9"             # optional pin, see below
```

Table names (`p2710s`) are labels for your own benefit; nothing matches on them.

`transform` accepts `normal`, `90`, `180`, `270`, `flipped`, `flipped-90`,
`flipped-180`, `flipped-270`.

`mode` is required rather than defaulting to the monitor's preferred mode —
preferred is frequently *not* the fastest mode (this machine's P2710S prefers
60 Hz and runs at 240 Hz), so a default would silently downgrade refresh rate.
`export` fills it in from the live state.

## Why identity, not connector

Connector names are not stable. Between the design notes for this tool and its
first run, the same physical panel moved from `DP-5` to `DP-9` and the second
output from `HDMI-1` to `HDMI-2` — no hardware was unplugged. Keying on
`(vendor, product, serial)` and resolving the connector at apply time survives
kernel, GPU and dock topology changes.

Mutter's own monitor spec is the four-field tuple
`(connector, vendor, product, serial)` and it matches on all four, which is
exactly why `monitors.xml` accumulates duplicate blocks for one physical panel
under different connector names. Stale blocks are inert — they can never match —
so cleaning them up is tidiness, not correctness.

**`connector` as a tie-breaker.** Empty or shared EDID serials are common:
identical panels from one batch can be indistinguishable by identity. When two
declared screens share an identity, or one identity matches two connected
monitors, add `connector` to pin it. Note that a pin reintroduces the
instability you were avoiding — a pinned screen whose connector was renamed
reads as "not connected" and `apply` becomes a no-op.

## Semantics worth knowing

- **`gdctl set` is declarative, not incremental.** Every invocation describes
  the complete desired state. Any connected monitor not assigned to a logical
  monitor is switched off — that is how you disable a screen, there is no
  `--off`. `apply` warns for each connected monitor missing from the layout.
- **`--persistent` is the default here, deliberately.** Without it, the change
  is never written to `monitors.xml` and is lost on hotplug, DPMS wake, the next
  apply, or the next login — while the previously stored block stays
  authoritative. A non-persistent tool silently loses to a stale stored config
  at every login.
- **`--persistent` and `--verify` are mutually exclusive**; Mutter rejects a
  config that is both. `--verify` therefore drops persistence, which costs
  nothing since it applies nothing.
- **Connected ≠ active.** A monitor appears in `GetCurrentState`'s `monitors`
  array even when it is disabled or leased. `export` reads the layout from
  `logical_monitors` and notes any connected-but-inactive monitor it omitted.
- **`--for-lease-monitor` is not "off"** — it hands the output to a DRM-leasing
  client (VR). Not modelled here.
- **State comes from D-Bus, never from `gdctl show`**, whose output is
  human-facing and reformattable.

## Deliberately not a systemd unit

A oneshot-at-login unit would duplicate Mutter's own match-and-apply and create
a second source of truth that can drift. It also does not fire on hotplug (udev
fires too early), a system unit has no session bus, and even a user unit ordered
against `graphical-session.target` races Mutter claiming the bus name.

The arrangement instead: TOML is the source of truth, `haichi apply` writes
`monitors.xml`, and Mutter handles login and hotplug from there — `monitors.xml`
is the compiled artifact.

If hotplug ever picks the wrong layout, the upgrade path is a long-running
listener on `MonitorsChanged` (kanshi-shaped). That signal carries no payload,
so it means "re-run `GetCurrentState`".

## Limitations

- **Mirroring is not expressible.** One logical monitor driving several outputs
  has no representation in this schema; `export` warns and emits the first
  output only.
- **Relative placement** (`--right-of`, `--above`, …) is not exposed; positions
  are absolute `x`/`y`. Mutter still enforces adjacency, so `--verify` is the
  cheap way to check a layout before it blanks your screens.
- **One layout per file.** For several hardware combinations, keep one file each
  and run them in sequence — each is a no-op unless its screens are present.
- **The GDM greeter** keeps its own `/var/lib/gdm/.config/monitors.xml`
  (`chown gdm:gdm`, `restorecon` on SELinux). Out of scope.
- `color-mode`, `rgb-range` and luminance (`gdctl pref`) are not modelled.

## Rebuilding `monitors.xml`

It is a generated cache, not a file to author. Mutter parses it once at startup
into an in-memory store and re-serialises the whole store on each persistent
apply — so deleting it mid-session and re-applying just rewrites the stale
entries. A real rebuild is: move the file aside → **full logout** (no `Alt+F2 r`
since GNOME 49 dropped X11) → re-apply each hardware combination.
