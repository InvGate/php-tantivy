<?php

declare(strict_types=1);

namespace Tantivy;

interface ClientInterface
{
    public static function openOrCreate(array $config): self;

    public static function openReadOnly(array $config): self;

    public function addDocument(array $doc): void;

    public function updateDocument(string $keyField, string $keyValue, array $doc): void;

    public function deleteDocument(string $keyField, string $keyValue): void;

    public function commit(): void;

    public function optimize(): void;

    public function documentCount(): int;

    /** @return list<array{score: float, fields: array<string, string>}> */
    public function search(array $query): array;

    public function close(): void;
}
