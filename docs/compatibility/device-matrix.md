# Unscroll V1 device-compatibility matrix

The broad V1 claim ("capability-tested Android 7-16 phones") requires the
full emulator matrix below **plus** observed evidence on all three hardware
classes: one Pixel-class device, one Samsung device, and one
Xiaomi/Redmi-class device. Missing evidence narrows the published claim; it
never blocks on an assumption.

Hardware results stay local: they are curated into this table by hand from
the manual protocol (`manual-test-protocol.md`). There is no telemetry,
no upload, and no automatic collection of any kind.

Result values: `Not-run`, `Pass`, `Fail`, `Partial` (with a limitation note).

## Emulator matrix (CI-enforced, `launcher/emulator/` + `.github/workflows/ci.yml`)

| Runtime | install | bridge | catalog/icons | suspend/unsuspend | notification suppression | HOME shell | HOME chooser | app-ops | store restriction | recovery storage | protected facts | Limitation |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| API 24 emulator | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | No attached disposable emulator snapshot. |
| API 28 emulator | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | No attached disposable emulator snapshot. |
| API 29 emulator | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | No attached disposable emulator snapshot. |
| API 33 emulator | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | No attached disposable emulator snapshot. |
| API 36 emulator | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | Not-run | API 24 must run first; no attached disposable emulator snapshot. |

Deterministic fake-adapter coverage proves local JSON and human-summary
output, complete adapter routing, restoration attempts, and failed-restore
diagnostics. It is not device-runtime evidence.

## Hardware classes (manual protocol, results curated locally)

| Runtime | install | bridge | catalog/icons | suspend/unsuspend | notification suppression | HOME shell | HOME chooser | app-ops | store restriction | recovery storage | protected facts | Limitation |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Pixel-class | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | No hardware claim. |
| Samsung | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | No hardware claim. |
| Xiaomi/Redmi | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | Not observed | No hardware claim. |

## Release-blocking rule

Failures in required store suspension, bridge security, recovery
reconciliation, or HOME verification block release. They are never marked
flaky. Exactly one retry is permitted for documented infrastructure flakes
only (emulator boot or harness timeout with no test assertion recorded).
