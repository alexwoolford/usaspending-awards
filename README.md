# usaspending-awards

Prime-award transactions whose recipient UEI is already on the exact CIK map. Contracts, and grants including cooperative agreements, for the current federal fiscal year and the one before it. The pair is taken from the UTC day the job runs. A year that falls behind is retracted on the next successful run.

The UEI-to-CIK decision stays in [uei-cik](https://github.com/alexwoolford/uei-cik). This tile does not read `recipient_parent_uei`.

```bash
cargo run --release -- ingest \
  --uei-db /var/lib/uei-cik/uei-cik.sqlite \
  --db data/usaspending-awards.sqlite \
  --cache-dir data/archive
cargo run --release -- status --db data/usaspending-awards.sqlite
cargo run --release -- lookup --db data/usaspending-awards.sqlite MJENSFHZJP11
```

Spec: [mosaic `docs/research/USASPENDING_AWARDS.md`](https://github.com/alexwoolford/mosaic/blob/main/docs/research/USASPENDING_AWARDS.md). Capture: [docs/CAPTURE.md](docs/CAPTURE.md).
