```bash

# Build image (once)
docker build --target rust-dav1d-builder -t dav1d-musl-builder .

# Build x86_64 — offline
docker run --rm \
  -v "$(pwd)":/usr/src/app \
  -v cargo-cache:/root/.cargo \
  -v rustup-cache:/root/.rustup \
  -w /usr/src/app \
  dav1d-musl-builder \
  cargo build --release --target x86_64-unknown-linux-musl

# Build aarch64 — offline
docker run --rm \
  -v "$(pwd)":/usr/src/app \
  -v cargo-cache:/root/.cargo \
  -v rustup-cache:/root/.rustup \
  -w /usr/src/app \
  dav1d-musl-builder \
  cargo build --release --target aarch64-unknown-linux-musl


# build i686 for wasm
# 
docker run --rm   -v "$(pwd)":/usr/src/app   -v cargo-cache:/root/.cargo   -v rustup-cache:/root/.rustup   -w /usr/src/app   dav1d-musl-builder   sh -c "rustup target add i686-unknown-linux-musl && RUSTFLAGS='-C target-feature=+sse,+sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx -C overflow-checks=false' cargo build --profile musl-release --target i686-unknown-linux-musl"

#For it to work add this to Cargo.toml

[profile.release]
opt-level = "s"
overflow-checks = false

# Add this to force dependencies to also drop overflow checks
[profile.release.package."*"]
overflow-checks = false

```
