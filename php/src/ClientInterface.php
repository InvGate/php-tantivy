<?php

declare(strict_types=1);

namespace Tantivy;

/**
 * Public contract for a tantivy index handle.
 *
 * All payloads are plain PHP arrays with a stable shape (mirrored 1:1 by the Rust core). The shapes
 * below are the source of truth for consumers; the backend ignores unknown keys rather than
 * erroring, so a typo silently does nothing — mind the exact names.
 *
 * Field buckets (set once at creation, fixed for the life of the index directory):
 *   - `text`       → tokenized + stored (full-text search targets).
 *   - `keys`       → stored, NOT tokenized (exact-match filters / ids).
 *   - `attributes` → stored only, not indexed (returned in hits, never searched).
 *
 * Durability (near-real-time): writes are buffered and become visible/durable only after
 * {@see commit()}. Closing the client (or letting it go out of scope) does NOT commit — any writes
 * since the last commit() are discarded. Commit explicitly before releasing the client.
 *
 * @phpstan-type IndexConfig array{
 *     path: string,
 *     id_field: string,
 *     fields: array{text?: list<string>, keys?: list<string>, attributes?: list<string>},
 *     writer_heap_bytes?: int
 * }
 * @phpstan-type Document array<string, scalar>
 * @phpstan-type WhereClause array{field: string, value: string, occur?: 'must'|'must_not'|'should'}
 * @phpstan-type InClause array{field: string, values: list<string>}
 * @phpstan-type SearchQuery array{
 *     text?: string,
 *     text_fields?: list<string>,
 *     where?: list<WhereClause>,
 *     in?: list<InClause>,
 *     limit?: int,
 *     min_score?: float
 * }
 * @phpstan-type Hit array{score: float, fields: array<string, string>}
 */
interface ClientInterface
{
    /**
     * Opens the index at `$config['path']`, creating it (and the directory) if absent. The schema is
     * derived from `fields`; reopening an existing index with an incompatible schema fails.
     *
     * @param IndexConfig $config
     *
     * @throws TantivyException if the index cannot be opened or created.
     */
    public static function openOrCreate(array $config): self;

    /**
     * Opens an EXISTING index read-only (for searching). Does not create the directory or index and
     * opens no writer, so it needs no write permissions and fails loudly on a missing index instead
     * of silently returning an empty one.
     *
     * @param IndexConfig $config
     *
     * @throws TantivyException if the index does not exist or cannot be opened.
     */
    public static function openReadOnly(array $config): self;

    /**
     * Buffers a document for indexing. Not visible until {@see commit()}. Keys not present in the
     * schema are ignored; scalar values are coerced to strings.
     *
     * @param Document $doc
     *
     * @throws IndexBusyException if another process holds the exclusive writer lock (e.g. a rebuild).
     * @throws TantivyException   on any other failure.
     */
    public function addDocument(array $doc): void;

    /**
     * Replaces every document whose `$keyField` equals `$keyValue` with `$doc` (delete-by-term + add
     * in one batch). Not visible until {@see commit()}.
     *
     * @param Document $doc
     *
     * @throws IndexBusyException if another process holds the exclusive writer lock.
     * @throws TantivyException   on any other failure.
     */
    public function updateDocument(string $keyField, string $keyValue, array $doc): void;

    /**
     * Deletes every document whose `$keyField` equals `$keyValue`. Not visible until {@see commit()}.
     *
     * @throws IndexBusyException if another process holds the exclusive writer lock.
     * @throws TantivyException   on any other failure.
     */
    public function deleteDocument(string $keyField, string $keyValue): void;

    /**
     * Flushes buffered writes to disk (fsync) and reloads the reader, making prior add/update/delete
     * calls durable and visible to searches. Expensive — batch writes between commits.
     *
     * @throws TantivyException if the commit or reader reload fails.
     */
    public function commit(): void;

    /**
     * Requests a segment merge/optimize. Currently a no-op (merges are scheduled elsewhere); kept for
     * API stability. Never fails.
     *
     * @throws TantivyException reserved for future failures.
     */
    public function optimize(): void;

    /**
     * Number of committed, non-deleted documents. Reflects only what has been {@see commit()}ed.
     *
     * @throws TantivyException if the count cannot be read.
     */
    public function documentCount(): int;

    /**
     * Runs a search and returns the matching hits, highest score first.
     *
     * Query semantics: each token of `text` must match (as exact OR fuzzy OR prefix) in at least one
     * of `text_fields`; `where` adds term filters with per-clause occur; `in` adds a
     * match-any-of-values filter. `limit` defaults to 20, is clamped to a sane maximum, and `0`
     * returns no hits. `text` is capped to the first 32 tokens (the rest are discarded). A query with
     * no effective clauses returns an empty list.
     *
     * @param SearchQuery $query
     *
     * @return list<Hit>
     *
     * @throws TantivyException if the query is malformed or the search fails.
     */
    public function search(array $query): array;

    /**
     * Releases the index handle (idempotent). Does NOT commit — see the durability note above.
     *
     * @throws TantivyException if closing fails.
     */
    public function close(): void;
}
