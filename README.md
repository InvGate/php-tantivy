# php-tantivy

Embed the [tantivy](https://github.com/quickwit-oss/tantivy) search engine (Rust) in PHP.

The Rust engine is exposed to PHP through **two interchangeable backends** that share the
same core, so you can pick whichever your environment allows:

| Backend | Loaded via | Needs `ffi.enable`? | Artifact |
|---|---|---|---|
| **FFI** | `FFI::cdef` | yes | `libtantivyphp.so` / `tantivyphp.dll` (plain cdylib, PHP-version-independent) |
| **Native extension** | `extension=` in `php.ini` | **no** | `libtantivyphp_ext.so` / `tantivyphp_ext.dll` (built with [ext-php-rs](https://github.com/davidcole1340/ext-php-rs)) |

Use the FFI backend when FFI is available; use the native extension when a hardened
environment forbids `ffi.enable`. Both are behaviorally identical — same engine, same
JSON boundary, same results.

## Requirements

- PHP **8.4**, NTS, x86_64 (Linux or Windows)
- For the FFI backend: `ffi.enable=1` and the cdylib on disk
- For the native extension: the extension loaded via `php.ini` (no FFI)

## Install / build

See [docs/BUILD.md](docs/BUILD.md). In short:

```bash
cargo build --release        # builds both libtantivyphp.so and libtantivyphp_ext.so
```

Then either:

```ini
; FFI backend
ffi.enable=1
; and point TANTIVYPHP_LIB / TANTIVYPHP_HEADER at the cdylib + include/tantivyphp.h
```

or:

```ini
; native extension backend
extension=tantivyphp_ext.so
```

## Usage

The PHP-facing API is a single facade, `Tantivy\Client`, which auto-selects the backend
(native extension if loaded, else FFI). Consumers depend only on `Tantivy\Client` and
`Tantivy\ClientInterface`.

```php
use Tantivy\Client;

$client = Client::openOrCreate([
    'path'      => '/var/lib/myindex',
    'id_field'  => 'id_key',
    'fields'    => [
        'text'       => ['title', 'body'],  // tokenized + searchable
        'keys'       => ['id_key'],         // exact-match keywords
        'attributes' => ['url'],            // stored, not indexed
    ],
    'writer_heap_bytes' => 50_000_000,
]);

$client->addDocument(['id_key' => '1', 'title' => 'reset password', 'body' => '...']);
$client->commit();                          // writes are near-real-time: commit to publish

$hits = $client->search([
    'text'        => 'passwrd',             // fuzzy
    'text_fields' => ['title', 'body'],
    'where'       => [['field' => 'id_key', 'value' => '1', 'occur' => 'must']],
    'limit'       => 10,
]);
// => [ ['score' => 1.2, 'fields' => ['id_key' => '1', ...]], ... ]
```

### Near-real-time semantics

Writes (`addDocument` / `updateDocument` / `deleteDocument`) are buffered and become
visible only after `commit()`. This mirrors how most search engines separate indexing
from refresh; batch your writes and commit periodically for throughput.

## Layout

```
crates/tantivy-core   Rust engine (schema, index registry, writer, query) — binding-agnostic
crates/tantivy-ffi    C-ABI cdylib (the FFI backend)
crates/tantivy-ext    ext-php-rs native extension (the extension backend)
include/tantivyphp.h  C header for the FFI backend
php/src               PHP client: Client (facade), ClientInterface, FfiClient, ExtClient
php/tests/smoke.php   backend-agnostic smoke test
```

## Testing

```bash
cargo test                                                    # Rust core + FFI roundtrip
TANTIVYPHP_LIB=target/release/libtantivyphp.so php -d ffi.enable=1 php/tests/smoke.php
php -d extension=target/release/libtantivyphp_ext.so php/tests/smoke.php
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
