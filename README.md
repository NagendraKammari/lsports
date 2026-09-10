# lsports

See what's listening on your ports — and why it's there.

`lsof -nP -iTCP:3000 -sTCP:LISTEN` tells you a `node` process holds port 3000.
It does not tell you *which* of your six checkouts it came from, when it
started, or how it got launched — which is what you actually wanted to know.

```console
$ lsports 3000
:3000  node
       pid 48213 · up 4h12m · 127.0.0.1 only
       cwd  ~/work/api-gateway  (git: feat/rate-limit)
       via  npm ← zsh
       cmd  node server.js
```

That's the whole idea. Same data everyone else has, plus the context that
identifies the process.

## Install

```console
cargo install lsports
```

Or grab a binary from [Releases](../../releases).

## Use

```console
$ lsports                       # everything you can see
:3000   node                     ~/work/api-gateway (feat/rate-limit)  up 4h12m
:5432   postgres (homebrew)      /opt/homebrew/var/postgresql@16       up 9d21h
:6379   redis-server (homebrew)  /opt/homebrew/var/db/redis            up 9d21h
:8080   python3                  /private/tmp/scratch                  up 1m ⚠ exposed

12 more listening socket(s) belong to other users — re-run with sudo to identify them
```

```console
$ lsports 3000 8080             # several ports
$ lsports --long                # full detail for every row
$ lsports --json                # machine-readable
```

### It tells you when a port is exposed

A dev server bound to `0.0.0.0` is reachable by everyone on your network —
the coffee shop wifi included. That gets its own marker rather than being
buried in an address column:

```console
:8080   python3   /private/tmp/scratch   up 1m ⚠ exposed
```

### It admits what it cannot see

Without root, the kernel will not say which process owns another user's socket,
so roughly half the listeners on a typical machine are unattributable. Other
tools quietly omit them, which makes "nothing is listening on port 22" a lie.
`lsports` distinguishes the two cases:

```console
$ lsports 22
port 22 is listening, but owned by another user — re-run with sudo to identify

$ lsports 9999
nothing is listening on port 9999
```

## Compared to what you're using now

|  | `lsof` / `netstat` | `lsports` |
|---|---|---|
| port → pid | yes | yes |
| working directory | no | yes |
| git branch | no | yes |
| uptime | via `ps`, separately | yes |
| launch chain | no | yes |
| exposed-binding warning | no | yes |
| distinguishes "free" from "hidden" | no | yes |
| memorable invocation | `lsof -nP -iTCP:3000 -sTCP:LISTEN` | `lsports 3000` |

## Notes

- **Platforms:** macOS, Linux, Windows. The "N more sockets" and blocked-port
  checks shell out to `ss` or `netstat` and are Unix-only; everything else is
  native.
- **Git branch** is read from `.git/HEAD` directly, and is deliberately *not*
  reported when the enclosing repository is a package manager's own checkout.
  A redis server whose cwd is `/opt/homebrew/var/db/redis` sits inside
  Homebrew's git repo, and calling that "branch: stable" is worse than saying
  nothing.
- **TCP only** for now.
- **MSRV** 1.86.

## Roadmap

- `lsports free 3000` — graceful `SIGTERM`, confirm before `SIGKILL`
- `lsports --wait-for-free 3000` — block until a port is released, for scripts
- container attribution (Docker, Colima, Podman)
- UDP

## License

MIT
