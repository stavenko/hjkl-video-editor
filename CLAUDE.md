# Project: hjkl-video-editor

GPU-accelerated, node-based video editor.

- **Backend**: Rust + Actix Web. Runs on a GPU machine. Requires `ffmpeg` on PATH.
- **Frontend**: Rust + Leptos 0.6 (CSR), built with Trunk.
- **Shared types**: `common/api-types` crate, used by both sides.

## Layout

```
backend/        actix server, configured via TOML
frontend/       leptos CSR + trunk
common/api-types/   request/response DTOs shared across the wire
```

## Backend architecture

- `config.rs` — TOML config (addr, port, projects root, ffmpeg binary).
- `providers/` — IO-facing components (filesystem, ffmpeg, …).
- `use_cases/` — one module per business operation. Each defines `Input`, `Output`, `Error`, and a `command(...)` async fn.
- `api/endpoints/` — thin actix handlers; one module per endpoint. They unwrap inputs, call a use case, wrap result in `ApiResponse`.
- `api/configurator.rs` — wires endpoints into actix routing.
- `api/response.rs` — uniform `ApiResponse<T>` envelope (`{status: "ok", ...}` on success; `{code, message}` on error).
- `models/` — domain types (e.g. `Project`).

## Project storage

Each project lives at `<projects_root>/<uuid>/`. Metadata (`name`, timestamps, eventually node graph) lives in `<projects_root>/<uuid>/project.toml`. The directory name is the stable id; rename only changes metadata.

## Run

Backend:
```
cp config.example.toml config.toml
cargo run -p backend -- run --config config.toml
```

Frontend:
```
cd frontend && trunk serve
```

Trunk proxies `/api/*` to the backend on port 3001 (see `frontend/Trunk.toml`).

## Ground rules

- Never silently catch errors — propagate or panic with context.
- Never invent sample data unless explicitly asked.
- Don't add features beyond what was requested.
