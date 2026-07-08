# Build

The workspace produces two independent artifacts. Build both with `cargo build --release`.

## FFI backend — `libtantivyphp.so` / `tantivyphp.dll`

A plain cdylib loaded from PHP via `FFI::cdef`. PHP-version-independent.

### Linux (NTS)
```
cargo build --release            # -> target/release/libtantivyphp.so
```

### Windows (NTS)
```
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
# -> target/x86_64-pc-windows-msvc/release/tantivyphp.dll
```
Plain cdylib: no PHP SDK, no ext-php-rs, no nightly required.

### Verify
```
TANTIVYPHP_LIB=target/release/libtantivyphp.so php -d ffi.enable=1 php/tests/smoke.php
```

## Native extension — `libtantivyphp_ext.so` / `tantivyphp_ext.dll`

A native PHP extension built with `ext-php-rs`, loaded via `extension=`. No `ffi.enable`
needed. Links against the PHP ABI, so it is built per PHP minor (currently 8.4) and per
thread-safety mode (NTS).

### Linux (NTS)
Requires PHP 8.4 dev headers (`php-config` on PATH) and Clang.
```
cargo build --release -p tantivy-ext   # -> target/release/libtantivyphp_ext.so
```

### Windows (NTS)
Requires Rust **nightly** (vectorcall calling convention), MSVC (`cl.exe`) + `rust-lld`,
and a PHP 8.4 NTS SDK from windows.php.net.
```
cargo +nightly build --release -p tantivy-ext   # -> target/release/tantivyphp_ext.dll
```

### Verify
```
php -d extension=target/release/libtantivyphp_ext.so php/tests/smoke.php
```

`php/tests/smoke.php` is backend-agnostic: it uses the `Tantivy\Client` facade, which selects
the native extension when it is loaded and otherwise falls back to FFI.
