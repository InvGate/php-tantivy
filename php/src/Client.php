<?php

declare(strict_types=1);

namespace Tantivy;

/**
 * Fachada que selecciona el backend disponible. Es el ÚNICO lugar que sabe que existen dos
 * implementaciones; los consumidores sólo dependen de esta clase y de ClientInterface.
 * Cuando un backend gane a mediano plazo, se borra su clase perdedora y esta rama del if.
 */
final class Client
{
    public static function openOrCreate(array $config): ClientInterface
    {
        return (self::backend())::openOrCreate($config);
    }

    public static function openReadOnly(array $config): ClientInterface
    {
        return (self::backend())::openReadOnly($config);
    }

    /** @return class-string<ClientInterface> */
    private static function backend(): string
    {
        // detecta la extensión nativa por la clase que registra (más robusto que el nombre del módulo).
        return \class_exists('\\Tantivy\\Native\\Index') ? ExtClient::class : FfiClient::class;
    }
}
