<?php

declare(strict_types=1);

// Smoke runner independiente del backend: usa la fachada Tantivy\Client, que elige FFI o ext
// según qué haya cargado. Sale con código != 0 y un mensaje si algo falla.

require __DIR__ . '/../src/TantivyException.php';
require __DIR__ . '/../src/ClientInterface.php';
require __DIR__ . '/../src/FfiClient.php';
require __DIR__ . '/../src/ExtClient.php';
require __DIR__ . '/../src/Client.php';

use Tantivy\Client;

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

$backend = \class_exists('\\Tantivy\\Native\\Index') ? 'ext' : 'ffi';
fwrite(STDOUT, "smoke backend: $backend\n");

$c = Client::openOrCreate($config);
$c->addDocument(['id_key' => '1', 'title' => 'reset password']);
$c->commit();
assert_that($c->documentCount() === 1, 'documentCount debe ser 1 tras add+commit');

$hits = $c->search(['text' => 'pasword', 'text_fields' => ['title'], 'limit' => 5]);
assert_that(count($hits) === 1, 'búsqueda fuzzy debe traer 1 hit');
assert_that(($hits[0]['fields']['id_key'] ?? null) === '1', 'el hit debe ser id_key=1');

$c->deleteDocument('id_key', '1');
$c->commit();
assert_that($c->documentCount() === 0, 'documentCount debe ser 0 tras delete+commit');
$c->close();

fwrite(STDOUT, "SMOKE OK ($backend)\n");
exit(0);
