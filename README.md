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

**Durability:** closing the client — `close()`, its destructor, or simply letting it go
out of scope — does **not** commit. Any writes since the last `commit()` are discarded.
Always `commit()` before releasing the client if you need those writes persisted.

### Configuration

`openOrCreate` / `openReadOnly` take a config array. The field buckets are fixed for the
life of the index directory (set at creation):

| Key                 | Type                                              | Notes                                                     |
| ------------------- | ------------------------------------------------- | --------------------------------------------------------- |
| `path`              | `string` (required)                               | Index directory. Created by `openOrCreate`.               |
| `id_field`          | `string` (required)                               | Must be one of `fields.keys`; validated against schema.   |
| `fields.text`       | `list<string>`                                    | Tokenized **and** stored — full-text search targets.      |
| `fields.keys`       | `list<string>`                                    | Stored, **not** tokenized — exact-match filters / ids.    |
| `fields.attributes` | `list<string>`                                    | Stored only, not indexed — returned in hits, not searched.|
| `writer_heap_bytes` | `int` (optional, default `50_000_000`)            | Writer buffer size. Ignored for read-only handles.        |

### Search query

`search` takes a query array; all keys are optional:

| Key           | Type                                                            | Default | Meaning                                                                 |
| ------------- | -------------------------------------------------------------- | ------- | ----------------------------------------------------------------------- |
| `text`        | `string`                                                       | `""`    | Free text. Each token must match (exact **or** fuzzy **or** prefix).    |
| `text_fields` | `list<string>`                                                 | `[]`    | Which `text` fields to match against. Unknown names are skipped.        |
| `where`       | `list<{field, value, occur?}>`                                 | `[]`    | Term filter. `occur` ∈ `must` (default), `must_not`, `should`.          |
| `in`          | `list<{field, values}>`                                        | `[]`    | Match-any-of-values filter (required as a group).                       |
| `limit`       | `int`                                                          | `20`    | Max hits. `0` returns none; large values are clamped to a safe ceiling. |
| `min_score`   | `float`                                                        | `0.0`   | Drop hits scoring below this.                                           |

Notes: `text` is capped to its first **32 tokens** (extra tokens are discarded and a
warning is logged to stderr). A query with no effective clauses returns `[]`. Each hit is
`['score' => float, 'fields' => array<string, string>]` (stored fields only).

### Errors

All failures throw `Tantivy\TantivyException`. When a write (`addDocument` /
`updateDocument` / `deleteDocument`) fails because another process holds tantivy's
exclusive writer lock (e.g. a rebuild is running), it throws the subclass
`Tantivy\IndexBusyException` — catch it to degrade gracefully (skip the write, rely on a
later re-index) instead of failing hard. Catch `TantivyException` for everything else.

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
phpunit -c php/phpunit.xml                                     # with the ext loaded (see CI)
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
