# Build

The workspace produces one artifact: the native PHP extension. Build it with `cargo build --release`.

## Native extension — `libtantivyphp_ext.so` / `tantivyphp_ext.dll`

A native PHP extension built with `ext-php-rs`, loaded via `extension=`. Links against the PHP
ABI, so it is built per PHP minor (currently 8.4) and per thread-safety mode (NTS).

### Linux (NTS)
Requires PHP 8.4 dev headers (`php-config` on PATH) and Clang.
```
cargo build --release   # -> target/release/libtantivyphp_ext.so
```

### Windows (NTS)
Requires Rust **nightly** (vectorcall calling convention), MSVC (`cl.exe`) + `rust-lld`,
and a PHP 8.4 NTS SDK from windows.php.net.
```
cargo +nightly build --release   # -> target/release/tantivyphp_ext.dll
```

### Install
Drop the built library into your PHP `extension_dir` (rename to `tantivyphp.so` if you like)
and load it:
```ini
extension=tantivyphp_ext.so
```

### Verify
```
php -d extension=target/release/libtantivyphp_ext.so php/tests/smoke.php
```

`php/tests/smoke.php` uses the `Tantivy\Client` facade, which requires the native extension
to be loaded (it errors with an actionable message otherwise).
