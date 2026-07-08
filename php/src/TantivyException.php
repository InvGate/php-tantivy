<?php

declare(strict_types=1);

namespace Tantivy;

use RuntimeException;

class TantivyException extends RuntimeException
{
    /**
     * Builds the right exception for a backend operation failure. When the core marked the error as
     * "index busy" (the exclusive writer lock is held, e.g. a rebuild is running), returns an
     * {@see IndexBusyException} so callers can tell it apart by TYPE rather than by message text.
     * Any other failure yields a plain TantivyException.
     */
    public static function forOperation(string $operation, string $rawError, ?\Throwable $previous = null): self
    {
        $message = $operation . ' falló: ' . $rawError;
        if (\str_contains($rawError, 'index_locked:')) {
            return new IndexBusyException($message, 0, $previous);
        }
        return new self($message, 0, $previous);
    }
}
