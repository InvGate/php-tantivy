<?php

declare(strict_types=1);

// Bootstrap mínimo para PHPUnit: no hay `composer install` en este repo
// standalone, así que cargamos las clases directamente en lugar de depender
// del autoload PSR-4 declarado en composer.json.
require_once __DIR__ . '/../src/TantivyException.php';
require_once __DIR__ . '/../src/ClientInterface.php';
require_once __DIR__ . '/../src/FfiClient.php';
require_once __DIR__ . '/../src/ExtClient.php';
require_once __DIR__ . '/../src/Client.php';
