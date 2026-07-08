<?php

declare(strict_types=1);

// Smoke runner: usa la fachada Tantivy\Client sobre la extensión nativa.
// Sale con código != 0 y un mensaje si algo falla.

require __DIR__ . '/../src/TantivyException.php';
require __DIR__ . '/../src/IndexBusyException.php';
require __DIR__ . '/../src/ClientInterface.php';
require __DIR__ . '/../src/ExtClient.php';
require __DIR__ . '/../src/Client.php';

use Tantivy\Client;
use Tantivy\IndexBusyException;

function assert_that(bool $cond, string $msg): void
{
    if (!$cond) {
        fwrite(STDERR, "SMOKE FAIL: $msg\n");
        exit(1);
    }
}

$dir = sys_get_temp_dir() . '/tv_smoke_' . getmypid();
@array_map('unlink', glob("$dir/*") ?: []);
@rmdir($dir);

$config = [
    'path' => $dir,
    'id_field' => 'id_key',
    'fields' => [
        'text' => ['title'],
        'keys' => ['id_key'],
        'attributes' => [],
    ],
    'writer_heap_bytes' => 15_000_000,
];

$c = Client::openOrCreate($config);
$c->addDocument(['id_key' => '1', 'title' => 'reset password']);
$c->commit();
assert_that($c->documentCount() === 1, 'documentCount debe ser 1 tras add+commit');

$hits = $c->search(['text' => 'pasword', 'text_fields' => ['title'], 'limit' => 5]);
assert_that(count($hits) === 1, 'búsqueda fuzzy debe traer 1 hit');
assert_that(($hits[0]['fields']['id_key'] ?? null) === '1', 'el hit debe ser id_key=1');

// limit:0 no debe reventar (TopDocs::with_limit(0) paniquea): el core lo trata como "0 resultados".
$none = $c->search(['text' => 'reset', 'text_fields' => ['title'], 'limit' => 0]);
assert_that($none === [], 'limit:0 debe devolver [] sin paniquear');

// Contención del writer lock: `$c` ya tiene el writer (y su lock exclusivo del dir) tras el add.
// Un segundo cliente sobre el MISMO dir que intente escribir debe recibir IndexBusyException,
// no un error genérico — es la ruta de degradación por contención (feature de IndexBusyException).
$busy = Client::openOrCreate($config);
$threw = false;
try {
    $busy->addDocument(['id_key' => '2', 'title' => 'otro']);
} catch (IndexBusyException $e) {
    $threw = true;
}
assert_that($threw, 'un segundo writer sobre el mismo dir debe lanzar IndexBusyException');
$busy->close();

$c->deleteDocument('id_key', '1');
$c->commit();
assert_that($c->documentCount() === 0, 'documentCount debe ser 0 tras delete+commit');
$c->close();

fwrite(STDOUT, "SMOKE OK (ext)\n");
exit(0);
