<?php

declare(strict_types=1);

namespace Tantivy\Tests;

use PHPUnit\Framework\TestCase;
use Tantivy\Client;

final class ClientTest extends TestCase
{
    private string $dir;

    protected function setUp(): void
    {
        $this->dir = sys_get_temp_dir() . '/tv_php_' . getmypid();
        @exec('rm -rf ' . escapeshellarg($this->dir));
    }

    protected function tearDown(): void
    {
        @exec('rm -rf ' . escapeshellarg($this->dir));
    }

    public function test_add_search_delete_roundtrip(): void
    {
        $client = Client::openOrCreate([
            'path' => $this->dir,
            'id_field' => 'id_key',
            'fields' => [
                'text' => ['title'],
                'keys' => ['id_key'],
                'attributes' => [],
            ],
            'writer_heap_bytes' => 15_000_000,
        ]);

        $client->addDocument(['id_key' => '1', 'title' => 'reset password']);
        $client->commit();
        self::assertSame(1, $client->documentCount());

        $hits = $client->search([
            'text' => 'reset',
            'text_fields' => ['title'],
            'limit' => 5,
        ]);
        self::assertCount(1, $hits);
        self::assertSame('1', $hits[0]['fields']['id_key']);

        $client->deleteDocument('id_key', '1');
        $client->commit();
        self::assertSame(0, $client->documentCount());

        $client->close();
    }
}
