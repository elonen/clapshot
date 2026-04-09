"""Generic TTL-aware LRU cache.

A simple dictionary-based cache that evicts the least recently used
entry when full, and optionally expires entries after a time limit.
Used for Slack file IDs (no TTL) and channel name lookups (with TTL).
"""

import time
from collections import OrderedDict
from typing import TypeVar, Generic, Hashable

K = TypeVar("K", bound=Hashable)
V = TypeVar("V")


class TTLCache(Generic[K, V]):
    """Bounded cache with least-recently-used eviction and optional expiry.

    Args:
        maxsize: Maximum number of entries before the oldest is evicted.
        ttl: Time-to-live in seconds. 0 means entries never expire.
    """

    def __init__(self, maxsize: int = 256, ttl: float = 0):
        self._maxsize = maxsize
        self._ttl = ttl
        # Each value is stored as (value, timestamp) so we can check expiry.
        self._data: OrderedDict[K, tuple[V, float]] = OrderedDict()


    def get(self, key: K) -> V | None:
        if key not in self._data:
            return None
        val, ts = self._data[key]

        # Expired? Remove and treat as a miss.
        if self._ttl and (time.monotonic() - ts) > self._ttl:
            del self._data[key]
            return None

        # Mark as recently used.
        self._data.move_to_end(key)
        return val


    def put(self, key: K, value: V) -> None:
        if key in self._data:
            self._data.move_to_end(key)
        self._data[key] = (value, time.monotonic())

        # Evict oldest entries if over capacity.
        while len(self._data) > self._maxsize:
            self._data.popitem(last=False)
