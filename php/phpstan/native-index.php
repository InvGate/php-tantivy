<?php

namespace Tantivy\Native;

/**
 * PHPStan stub. The real class is registered at runtime by the native `tantivyphp` extension, so it
 * is invisible to static analysis. Signatures mirror the `#[php(name = ...)]` methods in
 * crates/tantivy-ext/src/lib.rs — keep them in sync if the ext surface changes.
 */
final class Index
{
    public static function openOrCreate(string $configJson): self {}

    public static function openReadOnly(string $configJson): self {}

    public function addDocument(string $docJson): void {}

    public function updateDocument(string $keyField, string $keyValue, string $docJson): void {}

    public function deleteDocument(string $keyField, string $keyValue): void {}

    public function commit(): void {}

    public function optimize(): void {}

    public function docCount(): int {}

    public function search(string $queryJson): string {}

    public function close(): void {}
}
