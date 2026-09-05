# Compatibility probe protocol

Run the development-only `unscroll_probe` only against an emulator snapshot whose complete state is disposable. It refuses to send any ADB command without `--disposable-snapshot`.

The probe uses the packaged development ADB adapter exclusively. It records API, model, fingerprint, launcher/fixture installation, shell-only bridge health and facts, catalog/protected facts, icon metadata/read, recovery access, app-op baseline/suppression/restore, fixture suspend/unsuspend/restore, store presence, HOME resolution/selection/restoration, and chooser availability.

Run API 24 before API 36. Do not run hardware probes until a recovery baseline exists; never use an existing user app as the fixture. A failed restore is a failed probe and retains its local JSON and summary. The probe sends no telemetry and writes only `probe-results/probe-results.json` and `probe-results/probe-summary.txt`.

`unscroll_probe` is a Cargo development binary, not a Tauri external binary or bundle resource; NSIS receives only the configured resource list.
