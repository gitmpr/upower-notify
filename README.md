# upower-notify

A lightweight, configurable battery monitor. It listens to UPower events via D-Bus and sends desktop notifications or executes commands when battery levels change.

![upower-notify screenshot](./assets/notification.png)

## Features

- **Event-Driven**: Subscribes directly to UPower D-Bus signals (`org.freedesktop.UPower`) with 0% CPU idle usage.
- **Configurable Thresholds**: Support for UPower warning levels (`low`, `critical`, `action`) as well as fine-grained percentage thresholds (e.g. 50%, 40%, 30%, 20%).
- **Notification Customization**: Custom text, icons, urgency (`low`, `normal`, `critical`), and auto-dismiss timeouts (including sticky `timeout = 0`).
- **Scriptable**: Run custom shell commands on specific battery events.

## Why `upower-notify`?

In monolithic Desktop Environments (GNOME, KDE) or integrated shell suites (DankMaterialShell, Noctalia), battery warning daemons are built into the session manager.

For minimal, composed Wayland window manager setups (such as `niri`, `sway`, or `hyprland` paired with notification daemons like `mako` or `dunst`), there is no default session battery daemon. `upower-notify` fills this gap as a lightweight, zero-polling, D-Bus event-driven notification daemon.

## Runtime Requirements

- `upower`
- A notification server (e.g., `dunst`, `mako`)

## Installation

```bash
git clone https://github.com/Guanran928/upower-notify.git
cd upower-notify
cargo install --path .
```

### Nix Flake

```bash
nix run github:Guanran928/upower-notify
```

## License

MIT
