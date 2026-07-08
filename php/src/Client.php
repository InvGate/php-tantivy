<?php

declare(strict_types=1);

namespace Tantivy;

/**
 * Punto de entrada público. Los consumidores dependen sólo de esta clase y de ClientInterface;
 * la implementación concreta (ExtClient, sobre la extensión nativa ext-php-rs) queda encapsulada.
 *
 * @phpstan-import-type IndexConfig from ClientInterface
 */
final class Client
{
    /**
     * @param IndexConfig $config
     *
     * @throws TantivyException if the extension is missing or the index cannot be opened/created.
     */
    public static function openOrCreate(array $config): ClientInterface
    {
        self::ensureExtensionLoaded();
        return ExtClient::openOrCreate($config);
    }

    /**
     * @param IndexConfig $config
     *
     * @throws TantivyException if the extension is missing or the index does not exist.
     */
    public static function openReadOnly(array $config): ClientInterface
    {
        self::ensureExtensionLoaded();
        return ExtClient::openReadOnly($config);
    }

    /**
     * La extensión nativa registra la clase Tantivy\Native\Index al cargarse. Si no está, fallamos
     * con un mensaje accionable en vez de dejar reventar un "class not found" opaco más abajo.
     */
    private static function ensureExtensionLoaded(): void
    {
        if (!\class_exists('\\Tantivy\\Native\\Index')) {
            throw new TantivyException(
                'La extensión nativa tantivyphp no está cargada (falta Tantivy\\Native\\Index). '
                . 'Cargala con extension=tantivyphp.so (o tantivyphp_ext.so) en tu php.ini.'
            );
        }
    }
}
