# PC → phone transfer speed: benchmark and improvement plan

Measured 3 November 2026 on the real devices. This is the "Speed" leftover in `PROGRESS.md`
("revisit buffer sizes on a gigabit + 5 GHz setup"), which can now be tested: the PC is on gigabit
and the phone on 5 GHz.

## Setup
- **PC:** desktop, Intel e1000e Ethernet at **1 Gbps** (a faulty cable had capped it at 100 Mbps; replaced).
- **Router:** Wi‑Fi 6 capable, 5 GHz network the 5 GHz network.
- **Phone:** Redmi Note 11 on 5 GHz, about 1 m from the router with nothing in between, charging,
  battery saver off (set B).
- **Apps compared:** Continuum (`continuumd send --to <id> FILE`) vs GSConnect/KDE Connect (`shareFile`).
- **Method:** random (incompressible) 2.5 GB file, apps alternated each round, phone copy deleted
  between runs. Speed = file size ÷ time from send command to last byte, measured from the PC's
  `eno1` transmit counter. Every run arrived at full size (2,684,354,560 bytes).

## Results

| Run | GSConnect | Continuum |
|---|---|---|
| Set A, round 1 (phone position not controlled) | 38.7 MB/s | 42.6 MB/s |
| Set A, round 2 | – (stopped) | 41.3 MB/s |
| Set B, round 1 (phone next to router) | 42.0 MB/s | 42.6 MB/s |
| Set B, round 2 | – (stopped) | 37.0 MB/s |
| **Average of 2.5 GB runs** | **40.4 MB/s (2 runs)** | **40.9 MB/s (4 runs)** |
| Single 5 GB run each (earlier) | 43.9 MB/s | 35.1 MB/s (slow first 10 s, dip to 29 MB/s at 80 s) |

Earlier on 2.4 GHz with the 100 Mbps cable: Continuum ~7.5 MB/s (PROGRESS.md), GSConnect 14.7 MB/s.

**Conclusion:** on gigabit + 5 GHz, Continuum and GSConnect are **about equal (~40 MB/s, ±3 MB/s
between runs)**. The single 5 GB run where GSConnect looked 25% faster was not reproduced. The
sample is small (6 runs), so treat differences under ~5 MB/s as noise.

Both apps land on the same number, so the shared limit may be the **phone's Wi‑Fi link**, not the
app. Check first: phone **Settings → Wi‑Fi → (your 5 GHz network) → Link speed**. Rough rule: real TCP throughput ≈
50–65% of link speed. If the link is ~433 Mbps, ~40 MB/s is already the ceiling and code changes
won't help much. If it is ~866 Mbps or higher, there is headroom and the plan below applies.

## Where Continuum's receive time goes (code reading, not yet profiled)

Phone receive loop, `android/app/src/main/java/io/github/sharjeelmazhar/continuum/share/FileReceiver.kt`
(`receiveFile`, about lines 103–160):

- One thread does every step in sequence for each 256 KB chunk (`BUFFER = 256 * 1024`, line 188):
  TLS read/decrypt → `writer.write` → `digest.update` (SHA‑256) → `onBytes(n)`.
- Every 8 MB (`CHECKPOINT = 8L shl 20`, line 189): `fileDescriptor.sync()` plus `save()`, which
  writes and renames a JSON state file. At 40 MB/s that is ~5 forced syncs per second, ~320 per 2.5 GB,
  and the network read waits during each one.
- Writes go through a MediaStore `ParcelFileDescriptor`, so on Android 11+ they may pass through the
  FUSE layer.
- PC side (`desktop/continuumd/src/transfer.rs`): `CHUNK = 256 << 10`, `CHECKPOINT_BYTES = 8 << 20`.
  The sender is unlikely to be the bottleneck (gigabit link, fast CPU).

## Plan (do in this order)

1. **Measure before changing anything.** Add debug timing to `receiveFile`: total time spent in
   network read, `writer.write`, `digest.update`, and `sync()`+`save()`, logged once per file
   (`adb logcat -s Continuum`). One 2.5 GB run then shows the real bottleneck. If read time dominates
   (waiting on the network), the Wi‑Fi link is the limit; stop here.
2. **Pipeline the receive loop.** A reader thread fills a bounded queue (e.g. 4–8 × 1 MB buffers)
   from the socket; a writer thread writes, hashes and checkpoints. The network keeps reading while
   storage is busy. Keep resume offsets correct: only checkpoint bytes the writer has synced.
3. **Checkpoint by time, not every 8 MB.** E.g. sync + `save()` at most every 2 s (or 64 MB, whichever
   comes first). Power-cut loss stays bounded (~2 s of data) with far fewer stalls.
4. **Bigger buffers.** Set `socket.receiveBufferSize` (e.g. 4 MB) before connecting, and use 1 MB
   read/write buffers on both sides (match `CHUNK` in `transfer.rs`).
5. **Throttle `onBytes` / notification updates** to a few per second if they are not already.
6. **Keep TLS and the SHA‑256 check.** They are features; after step 2 the hash runs off the network
   path.

## Acceptance test
- Repeat the benchmark below, at least 5 rounds, alternating apps.
- Target: Continuum average **≥ GSConnect average** on the same setup, every file byte-identical
  (SHA‑256 check passes), and resume after Wi‑Fi off mid-transfer still works.

## How to re-run the benchmark
From the repo root: `ROUNDS=5 scripts/bench-transfer.sh`. It creates the 2.5 GB test file in
`bench-out/` (gitignored), alternates GSConnect/Continuum, deletes the phone copy after each run (via
the GSConnect SFTP mount), and appends one line per run to `bench-out/results.csv`. If a power cut
stops it, run the same command again: finished runs are skipped and a half-written test file is rebuilt.
Before comparing a new build, move the old `bench-out/results.csv` aside so runs don't mix.

Needs the phone on 5 GHz Wi-Fi, GSConnect connected and Continuum paired. Device ids in the script are for
the current pairing (set `CT_ID`, `GS_ID`, `PHONE` and `NIC`; see the script header).

Raw data from 3 November 2026: `docs/benchmarks/data/2026-11-03-set-a.csv` and `2026-11-03-set-b.csv`.
