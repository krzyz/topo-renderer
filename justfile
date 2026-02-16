# run backend
backend:
    cargo run -p topo-backend --release

# run desktop version
desktop:
    cargo run -p topo-renderer-desktop --release

# run desktop version
desktop-debug $RUST_LOG="topo_renderer":
    cargo run -p topo-renderer-desktop

# build wasm
[working-directory: 'topo-renderer-web']
build-wasm:
    wasm-pack build . --target web -- -Z build-std=std,panic_abort

[working-directory: 'topo-renderer-web']
serve-wasm:
    python3 ../web/serve.py

[working-directory: 'topo-renderer-web']
publish:
    scp -r pkg/* nixcomplexity:/mnt/data1/topo/html/pkg

[working-directory: 'topo-renderer-web']
publish-staging:
    scp -r {index.html,pkg} nixcomplexity:/mnt/data1/topo-staging/html/
