# =============================================================================
# Multi-arch static dav1d + Rust builder (Arch Linux)
# No curl downloads - bypasses musl.cc GitHub IP bans!
# =============================================================================

# --------------------------- x86_64 build ---------------------------------
FROM messense/rust-musl-cross:x86_64-musl AS build-x86_64
ENV PATH="/usr/local/musl/bin:${PATH}"
RUN apt-get update && apt-get install -y nasm meson ninja-build git pkg-config && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 https://code.videolan.org/videolan/dav1d.git /dav1d-src
WORKDIR /build
RUN printf "%s\n" \
    "[host_machine]" \
    "system = 'linux'" \
    "cpu_family = 'x86_64'" \
    "cpu = 'x86_64'" \
    "endian = 'little'" \
    "" \
    "[binaries]" \
    "c = 'x86_64-linux-musl-gcc'" \
    "ar = 'x86_64-linux-musl-ar'" \
    "strip = 'x86_64-linux-musl-strip'" \
    "pkgconfig = 'pkg-config'" \
    "" \
    "[properties]" \
    "needs_exe_wrapper = false" > /cross.txt

RUN cp -r /dav1d-src . && meson setup build-dir --cross-file=/cross.txt --prefix=/usr/local/lib/x86_64-unknown-linux-musl \
    --default-library=static -Denable_tests=false -Denable_tools=false && \
    ninja -C build-dir && ninja -C build-dir install

# --------------------------- aarch64 build --------------------------------
FROM messense/rust-musl-cross:aarch64-musl AS build-aarch64
ENV PATH="/usr/local/musl/bin:${PATH}"
RUN apt-get update && apt-get install -y nasm meson ninja-build git pkg-config && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 https://code.videolan.org/videolan/dav1d.git /dav1d-src
WORKDIR /build
RUN printf "%s\n" \
    "[host_machine]" \
    "system = 'linux'" \
    "cpu_family = 'aarch64'" \
    "cpu = 'aarch64'" \
    "endian = 'little'" \
    "" \
    "[binaries]" \
    "c = 'aarch64-linux-musl-gcc'" \
    "ar = 'aarch64-linux-musl-ar'" \
    "strip = 'aarch64-linux-musl-strip'" \
    "pkgconfig = 'pkg-config'" \
    "" \
    "[properties]" \
    "needs_exe_wrapper = false" > /cross.txt

RUN cp -r /dav1d-src . && meson setup build-dir --cross-file=/cross.txt --prefix=/usr/local/lib/aarch64-unknown-linux-musl \
    --default-library=static -Denable_tests=false -Denable_tools=false && \
    ninja -C build-dir && ninja -C build-dir install

# --------------------------- i686 build -----------------------------------
FROM messense/rust-musl-cross:i686-musl AS build-i686
ENV PATH="/usr/local/musl/bin:${PATH}"
RUN apt-get update && apt-get install -y nasm meson ninja-build git pkg-config && rm -rf /var/lib/apt/lists/*

RUN git clone --depth 1 https://code.videolan.org/videolan/dav1d.git /dav1d-src
WORKDIR /build
RUN printf "%s\n" \
    "[host_machine]" \
    "system = 'linux'" \
    "cpu_family = 'x86'" \
    "cpu = 'i686'" \
    "endian = 'little'" \
    "" \
    "[binaries]" \
    "c = 'i686-linux-musl-gcc'" \
    "ar = 'i686-linux-musl-ar'" \
    "strip = 'i686-linux-musl-strip'" \
    "pkgconfig = 'pkg-config'" \
    "" \
    "[properties]" \
    "needs_exe_wrapper = false" > /cross.txt

RUN cp -r /dav1d-src . && meson setup build-dir --cross-file=/cross.txt --prefix=/usr/local/lib/i686-unknown-linux-musl \
    --default-library=static -Denable_tests=false -Denable_tools=false && \
    ninja -C build-dir && ninja -C build-dir install

# --------------------------- final builder (Arch) -------------------------
FROM archlinux:latest AS rust-dav1d-builder

# Sync keyring safely before installing packages (using official non-blocking mirrors)
RUN echo 'Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch' > /etc/pacman.d/mirrorlist && \
    echo 'Server = https://mirrors.kernel.org/archlinux/$repo/os/$arch' >> /etc/pacman.d/mirrorlist && \
    pacman -Sy --noconfirm archlinux-keyring && \
    pacman -Su --noconfirm && \
    pacman -S --noconfirm base-devel pkgconf openssl git curl musl

# rustup + musl targets
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl i686-unknown-linux-musl

# Copy compiled dav1d libs
COPY --from=build-x86_64  /usr/local/lib/x86_64-unknown-linux-musl   /x86_64-unknown-linux-musl
COPY --from=build-aarch64 /usr/local/lib/aarch64-unknown-linux-musl  /aarch64-unknown-linux-musl
COPY --from=build-i686    /usr/local/lib/i686-unknown-linux-musl     /i686-unknown-linux-musl

# Fix dav1d.pc paths
RUN sed -i -e 's|^prefix=.*|prefix=/aarch64-unknown-linux-musl|' -e 's|^libdir=.*|libdir=${prefix}/lib|' /aarch64-unknown-linux-musl/lib/pkgconfig/dav1d.pc && \
    sed -i -e 's|^prefix=.*|prefix=/x86_64-unknown-linux-musl|' -e 's|^libdir=.*|libdir=${prefix}/lib|' /x86_64-unknown-linux-musl/lib/pkgconfig/dav1d.pc && \
    sed -i -e 's|^prefix=.*|prefix=/i686-unknown-linux-musl|' -e 's|^libdir=.*|libdir=${prefix}/lib|' /i686-unknown-linux-musl/lib/pkgconfig/dav1d.pc

# Extract the toolchains straight out of /usr/local/musl from each build stage
COPY --from=build-x86_64  /usr/local/musl /opt/x86_64-linux-musl-cross
COPY --from=build-aarch64 /usr/local/musl /opt/aarch64-linux-musl-cross
COPY --from=build-i686    /usr/local/musl /opt/i686-linux-musl-cross

# Map toolchain binaries to PATH safely
RUN ln -sf /opt/x86_64-linux-musl-cross/bin/x86_64-linux-musl-* /usr/local/bin/ && \
    ln -sf /opt/aarch64-linux-musl-cross/bin/aarch64-linux-musl-* /usr/local/bin/ && \
    ln -sf /opt/i686-linux-musl-cross/bin/i686-linux-musl-* /usr/local/bin/

# environment
ENV PKG_CONFIG=/usr/bin/pkg-config \
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
    CARGO_ENCODED_RUSTFLAGS_x86_64_unknown_linux_musl="-L/opt/x86_64-linux-musl-cross/lib -L/x86_64-unknown-linux-musl/lib -C link-arg=--sysroot=/opt/x86_64-linux-musl-cross" \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/x86_64-linux-musl-gcc \
    CC_x86_64_unknown_linux_musl=/usr/local/bin/x86_64-linux-musl-gcc \
    \
    # i686
    PKG_CONFIG_PATH_i686_unknown_linux_musl=/i686-unknown-linux-musl/lib/pkgconfig \
    PKG_CONFIG_LIBDIR_i686_unknown_linux_musl=/i686-unknown-linux-musl/lib \
    PKG_CONFIG_SYSROOT_DIR_i686_unknown_linux_musl=/ \
    CARGO_ENCODED_RUSTFLAGS_i686_unknown_linux_musl="-L/opt/i686-linux-musl-cross/lib -L/i686-unknown-linux-musl/lib -C link-arg=--sysroot=/opt/i686-linux-musl-cross" \
    CARGO_TARGET_I686_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/i686-linux-musl-gcc \
    CC_i686_unknown_linux_musl=/usr/local/bin/i686-linux-musl-gcc

WORKDIR /usr/src/app
