# Daily ops

Weekly ingest, after the Sunday UEI match. The oneshot downloads Award Data Archive Full files for the current federal fiscal year and the one before it, taken from the UTC day of the run, and keeps transactions whose `recipient_uei` is live in `/var/lib/uei-cik/uei-cik.sqlite`. A successful run retracts the year that fell behind.

| Unit | Schedule |
| --- | --- |
| `usaspending-awards-ingest.timer` | Sunday 18:00 UTC, 15m jitter, `Persistent=true` |
| `usaspending-awards-ingest.service` | oneshot, user `usaspend` |

Work sqlite: `/var/lib/usaspending-awards/usaspending-awards.sqlite`. Archive cache: `/var/lib/usaspending-awards/archive`. The map at `/var/lib/uei-cik/uei-cik.sqlite` is read-only. `usaspend` is in group `ueicik` and that directory is `775`, so SQLite can create the WAL shm beside the map. Logs: `journalctl -u usaspending-awards-ingest.service`.

A failed download records `status=error` and does not retract. `TimeoutStartSec=4h`.
