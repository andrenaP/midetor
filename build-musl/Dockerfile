# =============================================================================
# Multi-arch static dav1d + Rust builder (Arch Linux)
# =============================================================================
FROM messense/rust-musl-cross:x86_64-musl AS base

# ---- build tools ---------------------------------------------------------
RUN apt-get update && apt-get install -y \
    nasm meson ninja-build git pkg-config curl \
    && rm -rf /var/lib/apt/lists/*

# ---- aarch64 cross-toolchain (full tree) --------------------------------
RUN curl -L https://musl.cc/aarch64-linux-musl-cross.tgz \
    | tar -xz -C /opt && \
    ln -s /opt/aarch64-linux-musl-cross/bin/* /usr/local/bin/

# ---- clone dav1d ---------------------------------------------------------
RUN git clone --depth 1 https://code.videolan.org/videolan/dav1d.git /dav1d-src

# --------------------------- x86_64 build ---------------------------------
FROM base AS build-x86_64
ARG TARGET=x86_64-unknown-linux-musl
ARG CC=/usr/local/musl/bin/x86_64-unknown-linux-musl-gcc
ARG PREFIX=/usr/local/lib/$TARGET

RUN printf "%s\n" \
    "[host_machine]" \
    "system = 'linux'" \
    "cpu_family = 'x86_64'" \
    "cpu = 'x86_64'" \
    "endian = 'little'" \
    "" \
    "[binaries]" \
    "c = 'CC_PLACEHOLDER'" \
    "ar = 'AR_PLACEHOLDER'" \
    "strip = 'STRIP_PLACEHOLDER'" \
    "pkgconfig = 'pkg-config'" \
    "" \
    "[properties]" \
    "needs_exe_wrapper = false" \
    > /cross.txt && \
    sed -i "s|CC_PLACEHOLDER|$CC|g;" /cross.txt && \
    sed -i "s|AR_PLACEHOLDER|$(echo $CC | sed 's/gcc$/ar/')|g;" /cross.txt && \
    sed -i "s|STRIP_PLACEHOLDER|$(echo $CC | sed 's/gcc$/strip/')|g;" /cross.txt

WORKDIR /build
COPY --from=base /dav1d-src .
RUN meson setup build-dir --cross-file=/cross.txt --prefix=$PREFIX \
    --default-library=static -Denable_tests=false -Denable_tools=false && \
    ninja -C build-dir && ninja -C build-dir install && rm -rf build-dir

# --------------------------- aarch64 build --------------------------------
FROM base AS build-aarch64
ARG TARGET=aarch64-unknown-linux-musl
ARG CC=/opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc
ARG PREFIX=/usr/local/lib/$TARGET

RUN printf "%s\n" \
    "[host_machine]" \
    "system = 'linux'" \
    "cpu_family = 'aarch64'" \
    "cpu = 'aarch64'" \
    "endian = 'little'" \
    "" \
    "[binaries]" \
    "c = 'CC_PLACEHOLDER'" \
    "ar = 'AR_PLACEHOLDER'" \
    "strip = 'STRIP_PLACEHOLDER'" \
    "pkgconfig = 'pkg-config'" \
    "" \
    "[properties]" \
    "needs_exe_wrapper = false" \
    > /cross.txt && \
    sed -i "s|CC_PLACEHOLDER|$CC|g;" /cross.txt && \
    sed -i "s|AR_PLACEHOLDER|$(echo $CC | sed 's/gcc$/ar/')|g;" /cross.txt && \
    sed -i "s|STRIP_PLACEHOLDER|$(echo $CC | sed 's/gcc$/strip/')|g;" /cross.txt

WORKDIR /build
COPY --from=base /dav1d-src .
RUN meson setup build-dir --cross-file=/cross.txt --prefix=$PREFIX \
    --default-library=static -Denable_tests=false -Denable_tools=false && \
    ninja -C build-dir && ninja -C build-dir install && rm -rf build-dir

# --------------------------- final builder (Arch) -------------------------
FROM archlinux:latest AS rust-dav1d-builder

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm base-devel pkgconf openssl git curl musl

# rustup + musl targets
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add x86_64-unknown-linux-musl
RUN rustup target add aarch64-unknown-linux-musl

# dav1d libs
COPY --from=build-x86_64  /usr/local/lib/x86_64-unknown-linux-musl   /x86_64-unknown-linux-musl
COPY --from=build-aarch64 /usr/local/lib/aarch64-unknown-linux-musl  /aarch64-unknown-linux-musl

# Fix dav1d.pc
RUN sed -i \
    -e 's|^prefix=.*|prefix=/aarch64-unknown-linux-musl|' \
    -e 's|^libdir=.*|libdir=${prefix}/lib|' \
    /aarch64-unknown-linux-musl/lib/pkgconfig/dav1d.pc && \
    sed -i \
    -e 's|^prefix=.*|prefix=/x86_64-unknown-linux-musl|' \
    -e 's|^libdir=.*|libdir=${prefix}/lib|' \
    /x86_64-unknown-linux-musl/lib/pkgconfig/dav1d.pc

# full toolchains
COPY --from=base /usr/local/musl /usr/local/musl
COPY --from=base /usr/local/bin/x86_64-unknown-linux-musl-* /usr/local/bin/
COPY --from=base /opt/aarch64-linux-musl-cross /opt/aarch64-linux-musl-cross

# Fix x86_64 linker path
RUN ln -sf /usr/local/musl/bin/x86_64-unknown-linux-musl-gcc /usr/local/bin/x86_64-unknown-linux-musl-gcc
RUN ln -sf /opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc /usr/local/bin/aarch64-linux-musl-gcc

# environment
ENV \
    PKG_CONFIG=/usr/bin/pkg-config \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_ALL_STATIC=1 \
    \
    # aarch64
    PKG_CONFIG_PATH_aarch64_unknown_linux_musl=/aarch64-unknown-linux-musl/lib/pkgconfig \
    PKG_CONFIG_LIBDIR_aarch64_unknown_linux_musl=/aarch64-unknown-linux-musl/lib \
    PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_musl=/ \
    CARGO_ENCODED_RUSTFLAGS_aarch64_unknown_linux_musl="-L/opt/aarch64-linux-musl-cross/lib -L/aarch64-unknown-linux-musl/lib -C link-arg=--sysroot=/opt/aarch64-linux-musl-cross" \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/aarch64-linux-musl-gcc \
    CC_aarch64_unknown_linux_musl=/usr/local/bin/aarch64-linux-musl-gcc \
    \
    # x86_64
    PKG_CONFIG_PATH_x86_64_unknown_linux_musl=/x86_64-unknown-linux-musl/lib/pkgconfig \
    PKG_CONFIG_LIBDIR_x86_64_unknown_linux_musl=/x86_64-unknown-linux-musl/lib \
    PKG_CONFIG_SYSROOT_DIR_x86_64_unknown_linux_musl=/ \
    CARGO_ENCODED_RUSTFLAGS_x86_64_unknown_linux_musl="-L/usr/local/musl/lib -L/x86_64-unknown-linux-musl/lib" \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/x86_64-unknown-linux-musl-gcc \
    CC_x86_64_unknown_linux_musl=/usr/local/bin/x86_64-unknown-linux-musl-gcc

WORKDIR /usr/src/app
