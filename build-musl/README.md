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
```
