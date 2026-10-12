# Runs the backend and the page together.
[parallel]
dev: dev-api dev-web

# Rebuilds and restarts the backend on Rust changes.
dev-api:
    watchexec -r -w src -w Cargo.toml -- cargo run -- ui --port 7700 --no-open

# Serves the page with hot reload, proxying /api to dev-api.
dev-web:
    cd web && npm run dev
