<?php

declare(strict_types=1);

namespace Tantivy;

/**
 * Backend nativo (ext-php-rs). Wrapper delgado sobre la clase nativa Tantivy\Native\Index que
 * registra la extensión. Traduce cualquier error nativo a TantivyException para que el tipo de
 * excepción sea idéntico al del backend FFI. Mismo contrato JSON que FfiClient.
 */
final class ExtClient implements ClientInterface
{
    private function __construct(private readonly \Tantivy\Native\Index $index)
    {
    }

    public static function openOrCreate(array $config): ClientInterface
    {
        try {
            return new self(\Tantivy\Native\Index::openOrCreate(self::json($config)));
        } catch (\Throwable $e) {
            throw new TantivyException('open_or_create falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public static function openReadOnly(array $config): ClientInterface
    {
        try {
            return new self(\Tantivy\Native\Index::openReadOnly(self::json($config)));
        } catch (\Throwable $e) {
            throw new TantivyException('open_read_only falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function addDocument(array $doc): void
    {
        try {
            $this->index->addDocument(self::json($doc));
        } catch (\Throwable $e) {
            throw new TantivyException('add_document falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function updateDocument(string $keyField, string $keyValue, array $doc): void
    {
        try {
            $this->index->updateDocument($keyField, $keyValue, self::json($doc));
        } catch (\Throwable $e) {
            throw new TantivyException('update_document falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function deleteDocument(string $keyField, string $keyValue): void
    {
        try {
            $this->index->deleteDocument($keyField, $keyValue);
        } catch (\Throwable $e) {
            throw new TantivyException('delete_document falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function commit(): void
    {
        try {
            $this->index->commit();
        } catch (\Throwable $e) {
            throw new TantivyException('commit falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function optimize(): void
    {
        try {
            $this->index->optimize();
        } catch (\Throwable $e) {
            throw new TantivyException('optimize falló: ' . $e->getMessage(), 0, $e);
        }
    }

    public function documentCount(): int
    {
        try {
            return (int) $this->index->docCount();
        } catch (\Throwable $e) {
            throw new TantivyException('doc_count falló: ' . $e->getMessage(), 0, $e);
        }
    }

    /** @return list<array{score: float, fields: array<string, string>}> */
    public function search(array $query): array
    {
        try {
            $json = $this->index->search(self::json($query));
        } catch (\Throwable $e) {
            throw new TantivyException('search falló: ' . $e->getMessage(), 0, $e);
        }
        $decoded = json_decode($json, true);
        return $decoded['hits'] ?? [];
    }

    public function close(): void
    {
        try {
            $this->index->close();
        } catch (\Throwable $e) {
            throw new TantivyException('close falló: ' . $e->getMessage(), 0, $e);
        }
    }

    private static function json(array $data): string
    {
        return json_encode($data, JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
    }
}
