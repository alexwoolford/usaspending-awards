# Capture contract (work sqlite)

Decision: **capture prime-award transactions for mapped UEIs, not the archive and not the UEI-to-CIK rule.** Work sqlite is `{--db}` (prod `/var/lib/usaspending-awards/usaspending-awards.sqlite`). Logical name `usaspending-awards`. There is no published `current/` copy. The collector watches work sqlite only.

Canonical contract: [capturable-state design principles](https://github.com/alexwoolford/capturable-state/blob/main/docs/design-principles.md) and [datetime.md](https://github.com/alexwoolford/capturable-state/blob/main/docs/datetime.md).

Pin: `capturable-state` git tag `v0.1.1` (not a path dep).

Spec: [mosaic docs/research/USASPENDING_AWARDS.md](https://github.com/alexwoolford/mosaic/blob/main/docs/research/USASPENDING_AWARDS.md).

## What is captured

| Table / stream | Capture? | Mode | Why |
| --- | --- | --- | --- |
| `transactions` | **yes** | after | One prime-award transaction. Key `transaction_key` |
| `ingest_runs` | **yes** | after | Did the ingest finish? |
| `transaction_stage` | **no** | — | Scratch space until every Full file has been read |
| Archive zips, parent UEI, loans, direct payments | **no** | — | Not this tile |

A failed download records `ingest_runs.status=error` and does not retract. An identical transaction emits no extra `_outbox` row. A transaction that a later successful run no longer keeps gets `deleted_at` (Unix seconds).

Do not `collect --snapshot` this database.

## Clocks

| Layer | Columns | Type |
| --- | --- | --- |
| Facts | `action_date`, `as_of_date` | TEXT `YYYY-MM-DD` |
| Facts | run `started_at` / `finished_at` | TEXT `YYYY-MM-DDTHH:MM:SSZ` |
| Facts | `obligation` | TEXT decimal, sign preserved |
| Envelope | `_outbox.ts`, `deleted_at` | INTEGER Unix seconds |

## Announce / nudge

`install()` on work sqlite. Announce stem is `usaspending-awards`. Weekly oneshot: `usaspending-awards-ingest.timer`, Sunday 18:00 UTC, `TimeoutStartSec=4h`.
