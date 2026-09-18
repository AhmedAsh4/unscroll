#!/usr/bin/env bash
# Task 20 emulator-matrix runner: one required API level per invocation.
# Called by `.github/workflows/ci.yml` with exactly 24, 28, 29, 33, or 36.
# Emulator failures block: this script never retries. A single documented
# infra retry (emulator boot/timeout, not a test assertion) is owned by the
# CI step wrapper, never by this script. Store, bridge, reconciliation, and
# HOME failures are hard failures on every API level.
set -euo pipefail

API="${1:?usage: run-matrix.sh <24|28|29|33|36>}"
case "$API" in
  24|28|29|33|36) ;;
  *) echo "unsupported API level: $API (want 24, 28, 29, 33, or 36)" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/.."

# Host unit gate always runs first: filtering, bridge protocol, schema, and
# HOME intent unit tests must pass before any emulator boots.
./gradlew --no-daemon :app:testDebugUnitTest

# Managed-device instrumented gate for this API level: bridge DUMP security
# (shell-allowed / app-denied), recovery schema accept/reject, icon stream,
# and manifest/HOME capabilities on the live emulator image. The matrix
# devices are injected via the init script so no build file is modified.
# API 24 is below Gradle's managed-device minimum (26), so the documented
# opt-in is passed for every leg (it is a no-op on API 28+).
./gradlew --no-daemon -I emulator/init-managed-devices.gradle -Pandroid.experimental.testOptions.managedDevices.allowOldApiLevelDevices=true "pixel${API}DebugAndroidTest"
