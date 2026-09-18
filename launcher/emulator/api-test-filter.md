# Emulator API test filter (Task 20)

Each of the five required API levels (24, 28, 29, 33, 36) runs the same four
Android gates. The table maps each gate to the test classes that enforce it;
all four must pass on all five levels for the V1 compatibility claim.

| Gate | Unit tests (`:app:testDebugUnitTest`) | Instrumented tests (`pixel<api>DebugAndroidTest`) |
| --- | --- | --- |
| Launcher allowlist filtering | `PolicyFilterTest`, `ActivePolicyStoreTest` | Manifest/install behavior is covered by `ManifestCapabilitiesTest` |
| Bridge DUMP security (shell-allowed / app-denied) | `BridgeProtocolTest` | `BridgeProviderInstrumentationTest` (ordinary UID denied, shell `content call` allowed) |
| Recovery schema accept/reject | `RecoveryEnvelopeTest`, `PrivateEnvelopeStoreTest` | `BridgeProviderInstrumentationTest` (shell write/read of an isolated valid envelope; corrupt envelope rejected) |
| HOME intent handling | `LauncherIntentTest` | `ManifestCapabilitiesTest`, `BridgeProviderInstrumentationTest` (HOME resolution for the envelope fixture) |

API-specific notes (informational only; no gate is skipped on any level):

- API 24: minimum supported; no adaptive icons and no notification channels.
  `generatedAndAdaptiveIconsEncodeAsBoundedPortablePng` covers the
  non-adaptive path; icon PNGs stay bounded portable PNG either way.
- API 28/29: scoped-storage transition (29) and gestural navigation (29);
  filtering and HOME gates are behavior-identical across them.
- API 33: runtime notification permission; blocked-app notification
  suppression is observed on hardware (see `docs/compatibility/`), while the
  emulator gate pins suspension state, not notification shade contents.
- API 36: current target/compile SDK; full gate, no exemptions.

Failure policy: store suspension, bridge security, recovery reconciliation,
and HOME verification failures are hard failures and block release. They are
never marked flaky. Exactly one retry is permitted for documented
infrastructure flakes only (emulator boot or harness timeout with no test
assertion recorded); the retry is recorded in the workflow log.
