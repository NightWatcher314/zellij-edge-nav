# zellij-edge-nav

A tiny [Zellij](https://zellij.dev/) WASM plugin for Kitty/tmux-style pane navigation.

It **only checks** whether the currently focused tiled Zellij pane is on a requested edge. It does **not** move focus by itself.

This is useful when an outer terminal/window manager (Kitty, yabai, etc.) needs to decide whether to keep navigation inside Zellij or hand off to the outer window system.

## Output

When called with a direction, the plugin prints exactly one of:

- `edge` — the focused Zellij pane is already on that edge
- `inside` — there is another Zellij pane in that direction

Supported directions:

- `left`, `right`, `up`, `down`
- aliases: `l`, `r`, `u`, `d`, `west`, `east`, `north`, `south`

## Recommended installation: Zellij plugin alias

Add this to `~/.config/zellij/config.kdl`:

```kdl
plugins {
    edge-nav location="https://github.com/NightWatcher314/zellij-edge-nav/releases/latest/download/zellij_edge_nav.wasm"
}
```

Then query it with:

```sh
zellij action pipe --plugin edge-nav --name edge-nav -- left
```

Zellij will download/cache the plugin when needed.

> For reproducible installs, replace `latest` with a specific tag, for example:
>
> `https://github.com/NightWatcher314/zellij-edge-nav/releases/download/v0.1.0/zellij_edge_nav.wasm`

## Local wrapper script

Create `~/.local/bin/zellij-edge-nav`:

```bash
#!/usr/bin/env bash
set -euo pipefail

DIR=${1:-}
if [[ -z "$DIR" || ! "$DIR" =~ ^(left|right|up|down|l|r|u|d|west|east|north|south)$ ]]; then
  echo "usage: zellij-edge-nav <left|right|up|down> [session]" >&2
  exit 2
fi

SESSION=${2:-${ZELLIJ_SESSION_NAME:-}}
args=(zellij)
if [[ -n "$SESSION" ]]; then
  args+=(-s "$SESSION")
fi

exec "${args[@]}" action pipe --plugin edge-nav --name edge-nav -- "$DIR"
```

Make it executable:

```sh
chmod +x ~/.local/bin/zellij-edge-nav
```

## Example navigation logic

```sh
case "$(zellij-edge-nav left | tr -d '\n')" in
  edge)
    # Hand off to outer window manager / terminal.
    yabai -m window --focus west
    ;;
  inside)
    # Move inside Zellij yourself.
    zellij action move-focus left
    ;;
esac
```

## Kitty kitten example

In a Kitty kitten, the decision can be:

1. If foreground command is `zellij`, run `zellij-edge-nav <direction>`.
2. If it returns `inside`, send/run `zellij action move-focus <direction>` or send your Zellij key binding.
3. If it returns `edge`, use Kitty's `neighboring_window()` or your window-manager handoff.

## Manual local build

```sh
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
```

The plugin artifact will be:

```txt
target/wasm32-wasip1/release/zellij_edge_nav.wasm
```

You can also load it from a local path:

```sh
zellij action pipe \
  --plugin file:$PWD/target/wasm32-wasip1/release/zellij_edge_nav.wasm \
  --name edge-nav -- left
```

## Release process

Maintainers can publish a new release by pushing a semver tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions will build and attach:

- `zellij_edge_nav.wasm`
- `zellij_edge_nav.wasm.sha256`

## License

MIT
