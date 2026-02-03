# run backend
backend:
    cargo run -p topo-backend --release

# run desktop version
desktop:
    cargo run -p topo-renderer --release

# run desktop version
desktop-debug $RUST_LOG="topo_renderer":
    cargo run -p topo-renderer

# build wasm
[working-directory: 'topo-renderer']
build-wasm:
    wasm-pack build . --target web -- -Z build-std=std,panic_abort

[working-directory: 'topo-renderer']
serve-wasm:
    python3 ../web/serve.py

# build wasm
[working-directory: 'topo-renderer-web']
build-wasm2:
    wasm-pack build . --target web -- -Z build-std=std,panic_abort

[working-directory: 'topo-renderer-web']
serve-wasm2:
    python3 ../web/serve.py
