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

# Host unit gate + managed-device instrumented gate in one Gradle
# invocation: filtering, bridge protocol, schema, and HOME intent unit
# tests first, then the live-emulator gate for this API level (bridge
# DUMP security shell-allowed/app-denied, recovery schema accept/reject,
# icon stream, manifest/HOME capabilities). One invocation shares a
# single dependency-resolution pass, so the five concurrent legs do not
# trip Maven Central rate limits; without --continue a unit-gate failure
# still skips the emulator boot. The matrix devices are injected via the
# init script so no build file is modified.
./gradlew --no-daemon -I emulator/init-managed-devices.gradle -Pandroid.experimental.testOptions.managedDevices.allowOldApiLevelDevices=true :app:testDebugUnitTest "pixel${API}DebugAndroidTest"
