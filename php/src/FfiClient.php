<?php

declare(strict_types=1);

namespace Tantivy;

use FFI;

final class FfiClient implements ClientInterface
{
    private static ?FFI $ffi = null;

    private function __construct(private readonly int $handle)
    {
    }

    private static function ffi(): FFI
    {
        if (self::$ffi === null) {
            $header = file_get_contents(__DIR__ . '/../../include/tantivyphp.h');
            $lib = getenv('TANTIVYPHP_LIB');
            if ($lib === false) {
                $lib = __DIR__ . '/../../target/release/libtantivyphp.so';
            }
            // quita las directivas FFI_SCOPE/FFI_LIB del header para cdef
            $header = preg_replace('/^#define .*$/m', '', $header);
            self::$ffi = FFI::cdef($header, $lib);
        }
        return self::$ffi;
    }

    private static function lastError(): string
    {
        $ptr = self::ffi()->tv_last_error();
        if ($ptr === null) {
            return 'error desconocido';
        }
        $msg = FFI::string($ptr);
        self::ffi()->tv_string_free($ptr);
        return $msg;
    }

    public static function openOrCreate(array $config): ClientInterface
    {
        $handle = self::ffi()->tv_index_open_or_create(self::json($config));
        if ($handle === 0) {
            throw new TantivyException('open_or_create falló: ' . self::lastError());
        }
        return new self($handle);
    }

    /**
     * Abre un índice EXISTENTE en modo solo-lectura (para búsquedas). Falla si no existe:
     * no crea ni muta el índice, así una búsqueda no requiere permisos de escritura y un
     * índice no construido falla explícito en vez de crear uno vacío ("0 resultados").
     */
    public static function openReadOnly(array $config): ClientInterface
    {
        $handle = self::ffi()->tv_index_open_readonly(self::json($config));
        if ($handle === 0) {
            throw new TantivyException('open_read_only falló: ' . self::lastError());
        }
        return new self($handle);
    }

    public function addDocument(array $doc): void
    {
        if (self::ffi()->tv_add_document($this->handle, self::json($doc)) !== 0) {
            throw new TantivyException('add_document falló: ' . self::lastError());
        }
    }

    public function updateDocument(string $keyField, string $keyValue, array $doc): void
    {
        if (self::ffi()->tv_update_document($this->handle, $keyField, $keyValue, self::json($doc)) !== 0) {
            throw new TantivyException('update_document falló: ' . self::lastError());
        }
    }

    public function deleteDocument(string $keyField, string $keyValue): void
    {
        if (self::ffi()->tv_delete_document($this->handle, $keyField, $keyValue) !== 0) {
            throw new TantivyException('delete_document falló: ' . self::lastError());
        }
    }

    public function commit(): void
    {
        if (self::ffi()->tv_commit($this->handle) !== 0) {
            throw new TantivyException('commit falló: ' . self::lastError());
        }
    }

    public function optimize(): void
    {
        self::ffi()->tv_optimize($this->handle);
    }

    public function documentCount(): int
    {
        $n = self::ffi()->tv_doc_count($this->handle);
        if ($n < 0) {
            throw new TantivyException('doc_count falló: ' . self::lastError());
        }
        return (int) $n;
    }

    /**
     * @return list<array{score: float, fields: array<string, string>}>
     */
    public function search(array $query): array
    {
        $ptr = self::ffi()->tv_search($this->handle, self::json($query));
        if ($ptr === null) {
            throw new TantivyException('search falló: ' . self::lastError());
        }
        $json = FFI::string($ptr);
        self::ffi()->tv_string_free($ptr);
        $decoded = json_decode($json, true);
        return $decoded['hits'] ?? [];
    }

    public function close(): void
    {
        self::ffi()->tv_index_close($this->handle);
    }

    private static function json(array $data): string
    {
        return json_encode($data, JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
    }
}
