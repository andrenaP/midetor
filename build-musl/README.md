**Cross-Compiling with Docker**

We use a custom Arch Linux Docker container with pre-compiled static `dav1d` libraries to cross-compile for multiple architectures locally without configuring host toolchains.

### 1. Build the Builder Image (Run Once)

Because the Dockerfile is located in the `build-musl` directory, you must pass the `-f` flag from the root of the repository:

```bash
docker build --target rust-dav1d-builder -t dav1d-musl-builder -f build-musl/Dockerfile .

```

### 2. Compile for Target Architectures

The build commands map your local directory and cache Cargo dependencies, so subsequent builds are fast.

**Build x86_64 (64-bit Linux)**

```bash
docker run --rm \
  -v "$(pwd)":/usr/src/app \
  -v cargo-cache:/root/.cargo \
  -v rustup-cache:/root/.rustup \
  -w /usr/src/app \
  dav1d-musl-builder \
  cargo build --release --target x86_64-unknown-linux-musl

```

**Build aarch64 (ARM64 Linux)**

```bash
docker run --rm \
  -v "$(pwd)":/usr/src/app \
  -v cargo-cache:/root/.cargo \
  -v rustup-cache:/root/.rustup \
  -w /usr/src/app \
  dav1d-musl-builder \
  cargo build --release --target aarch64-unknown-linux-musl

```

**Build i686 (32-bit x86 for Wasm / CheerpX / v86)**

```bash
docker run --rm \
  -v "$(pwd)":/usr/src/app \
  -v cargo-cache:/root/.cargo \
  -v rustup-cache:/root/.rustup \
  -w /usr/src/app \
  dav1d-musl-builder \
  sh -c "RUSTFLAGS='-C target-feature=+sse,+sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx' cargo build --profile i686-release --target i686-unknown-linux-musl"
  
```
