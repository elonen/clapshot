import time
from typing import Optional


class FolderViewerTracker:
    """
    Tracks which session IDs are currently viewing each folder.
    Used to send refresh hints to non-acting viewers when folder contents change.

    Capped to avoid unbounded growth; evicts oldest entries when full.
    """
    MAX_ENTRIES = 10_000

    def __init__(self) -> None:
        # folder_id -> {sid: registration_timestamp}
        self._folder_to_sids: dict[int, dict[str, float]] = {}
        # sid -> folder_id  (reverse map for efficient unregister)
        self._sid_to_folder: dict[str, int] = {}

    def register(self, folder_id: int, sid: str) -> None:
        # Remove from previous folder if navigated
        self._unregister(sid)

        # Enforce cap by evicting oldest sid across all folders
        if len(self._sid_to_folder) >= self.MAX_ENTRIES:
            oldest_sid = min(
                self._sid_to_folder,
                key=lambda s: self._folder_to_sids.get(self._sid_to_folder[s], {}).get(s, 0)
            )
            self._unregister(oldest_sid)

        self._folder_to_sids.setdefault(folder_id, {})[sid] = time.monotonic()
        self._sid_to_folder[sid] = folder_id

    def _unregister(self, sid: str) -> None:
        old_folder = self._sid_to_folder.pop(sid, None)
        if old_folder is not None:
            sids = self._folder_to_sids.get(old_folder)
            if sids is not None:
                sids.pop(sid, None)
                if not sids:
                    del self._folder_to_sids[old_folder]

    def get_other_viewers(self, folder_id: int, exclude_sid: Optional[str]) -> list[str]:
        sids = self._folder_to_sids.get(folder_id, {})
        return [s for s in sids if s != exclude_sid]
