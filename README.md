# php-tantivy

Embed the [tantivy](https://github.com/quickwit-oss/tantivy) search engine (Rust) in PHP.

The Rust engine is exposed to PHP as a **native PHP extension** built with
[ext-php-rs](https://github.com/davidcole1340/ext-php-rs) and loaded via `extension=` in
`php.ini` — no `ffi.enable` required. Artifact: `libtantivyphp_ext.so` / `tantivyphp_ext.dll`.

## Requirements

- PHP **8.4**, NTS, x86_64 (Linux or Windows)
- The extension loaded via `php.ini`

## Install / build

See [docs/BUILD.md](docs/BUILD.md). In short:

```bash
cargo build --release        # builds target/release/libtantivyphp_ext.so
```

Then load it:

```ini
extension=tantivyphp_ext.so
```

## Usage

The PHP-facing API is a single facade, `Tantivy\Client`, backed by the native extension.
Consumers depend only on `Tantivy\Client` and `Tantivy\ClientInterface`.

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
crates/tantivy-ext    ext-php-rs native extension
php/src               PHP client: Client (facade), ClientInterface, ExtClient
php/tests/smoke.php   smoke test (requires the extension loaded)
```

## Testing

```bash
cargo test                                                    # Rust core + ext
php -d extension=target/release/libtantivyphp_ext.so php/tests/smoke.php
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
