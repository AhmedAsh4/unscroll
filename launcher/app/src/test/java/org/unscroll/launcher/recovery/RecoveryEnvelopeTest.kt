package org.unscroll.launcher.recovery

import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test
import java.io.File

class RecoveryEnvelopeTest {
    private fun fixture(name: String): String {
        var directory = File(System.getProperty("user.dir"))
        repeat(6) {
            val candidate = File(directory, "contracts/fixtures/recovery-v1/$name")
            if (candidate.isFile) return candidate.readText()
            directory = directory.parentFile ?: return@repeat
        }
        error("missing fixture: $name")
    }

    @Test
    fun acceptsSharedValidFixturesAndPreservesCanonicalBytes() {
        listOf(
            "valid/new-baseline.json",
            "valid/pending-mutation.json",
            "valid/applied-mutation.json",
            "valid/maintenance-open.json",
            "valid/strict-extension.json",
            "valid/stale-copy.json",
        ).forEach { name ->
            val envelope = RecoveryEnvelopeV1.parse(fixture(name))
            assertEquals(fixture(name).trimEnd(), envelope.canonicalJson())
            assertEquals(envelope.checksum, envelope.computedChecksum())
        }
    }

    @Test
    fun rejectsCorruptAndAmbiguousSharedFixtures() {
        mapOf(
            "invalid/checksum-failure.json" to ValidationError.CHECKSUM,
            "invalid/forked-history.json" to ValidationError.HISTORY,
            "invalid/baseline-mismatch.json" to ValidationError.BASELINE,
            "invalid/unknown-operation.json" to ValidationError.OPERATION,
        ).forEach { (name, expected) ->
            try {
                RecoveryEnvelopeV1.parse(fixture(name))
                fail("$name should reject")
            } catch (error: RecoveryValidationException) {
                assertEquals(expected, error.classification)
            }
        }
    }

    @Test
    fun rejectsBaselineMutationBetweenRevisions() {
        val older = RecoveryEnvelopeV1.parse(fixture("valid/pending-mutation.json"))
        val newer = RecoveryEnvelopeV1.parse(fixture("valid/applied-mutation.json"))
        older.requireStrictPrefixOf(newer)

        val mutated = RecoveryEnvelopeV1.parseUncheckedChecksum(fixture("invalid/baseline-mismatch.json"))
        try {
            older.requireStrictPrefixOf(mutated)
            fail("mutated baseline should reject")
        } catch (error: RecoveryValidationException) {
            assertEquals(ValidationError.BASELINE, error.classification)
        }
    }

    @Test
    fun rejectsHostileFieldsAndNonReversingInverses() {
        val valid = fixture("valid/applied-mutation.json")
        listOf(
            valid.replace("recovery-v1", "recovery-v2") to ValidationError.SCHEMA,
            valid.replace("com.social", "bad package") to ValidationError.FIELD,
            valid.replace("\"user_id\":0", "\"user_id\":1000000") to ValidationError.FIELD,
            valid.replace("\"checksum\":\"", "\"checksum\":\"z") to ValidationError.CHECKSUM,
            valid.replace("\"suspended\":false", "\"suspended\":true") to ValidationError.OPERATION,
            valid.replace("google/pixel/test", "x".repeat(513)) to ValidationError.FIELD,
        ).forEach { (input, expected) ->
            try {
                RecoveryEnvelopeV1.parse(input)
                fail("hostile input should reject")
            } catch (error: RecoveryValidationException) {
                assertEquals(expected, error.classification)
            }
        }
        try {
            RecoveryEnvelopeV1.parse(fixture("valid/strict-extension.json").replace("33333333-3333-3333-3333-333333333333", "22222222-2222-2222-2222-222222222222"))
            fail("duplicate operation ID should reject")
        } catch (error: RecoveryValidationException) {
            assertEquals(ValidationError.OPERATION, error.classification)
        }
        listOf(
            fixture("valid/pending-mutation.json").replace("\"revision\":1", "\"revision\":2"),
            fixture("valid/applied-mutation.json").replace("\"revision\":2", "\"revision\":1"),
        ).forEach { input ->
            try {
                RecoveryEnvelopeV1.parse(input)
                fail("impossible revision should reject")
            } catch (error: RecoveryValidationException) {
                assertEquals(ValidationError.HISTORY, error.classification)
            }
        }
    }

    @Test
    fun rejectsWellFormedEnvelopeBoundToAnotherDevice() {
        val input = fixture("valid/new-baseline.json")
        RecoveryEnvelopeV1.parseForDevice(input, "ABC123", "google/pixel/test", 0)
        try {
            RecoveryEnvelopeV1.parseForDevice(input, "OTHER", "google/pixel/test", 0)
            fail("foreign binding should reject")
        } catch (error: RecoveryValidationException) {
            assertEquals(ValidationError.DEVICE_BINDING, error.classification)
        }
    }
}
