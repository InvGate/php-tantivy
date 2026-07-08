<?php

declare(strict_types=1);

namespace Tantivy;

/**
 * The index is busy: another process holds tantivy's exclusive writer lock (typically a rebuild in
 * progress). This is a transient contention condition, not a data error — a writer may choose to
 * degrade (skip the write and rely on a later re-index) rather than fail hard.
 */
final class IndexBusyException extends TantivyException
{
}
