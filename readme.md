# Running desktop version

`cargo run --release`

# Running wasm version

`wasm-pack build --target web`

`python3 -m http.server 8080`

*Important* WebGPU only works on HTTPS and localhost, so use "localhost:8080" instead of "0.0.0.0:8080"!
