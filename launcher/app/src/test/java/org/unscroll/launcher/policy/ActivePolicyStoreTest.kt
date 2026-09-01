package org.unscroll.launcher.policy

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.unscroll.launcher.recovery.PrivateEnvelopeStore
import org.unscroll.launcher.recovery.RecoveryEnvelopeV1

class ActivePolicyStoreTest {
    private fun fixture(name: String): String {
        var directory = File(System.getProperty("user.dir") ?: error("missing user directory"))
        repeat(6) {
            val candidate = File(directory, "contracts/fixtures/recovery-v1/$name")
            if (candidate.isFile) return candidate.readText()
            directory = directory.parentFile ?: return@repeat
        }
        error("missing fixture: $name")
    }

    @Test
    fun `valid policy exposes its allowlist baseline and revision`() {
        val directory = Files.createTempDirectory("active-policy").toFile()
        try {
            PrivateEnvelopeStore(directory).write(RecoveryEnvelopeV1.parse(fixture("valid/applied-mutation.json")))

            val state = ActivePolicyStore(PrivateEnvelopeStore(directory)).current()

            assertEquals(ActivePolicy(setOf("com.phone"), "com.android.launcher3", 2), state)
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun `missing corrupt cleared and stale state needs recovery`() {
        val directory = Files.createTempDirectory("active-policy").toFile()
        try {
            val store = PrivateEnvelopeStore(directory)
            val policies = ActivePolicyStore(store)
            assertEquals(RecoveryNeeded, policies.current())

            File(directory, "recovery-v1.json").writeText("corrupt")
            assertEquals(RecoveryNeeded, policies.current())

            store.write(RecoveryEnvelopeV1.parse(fixture("valid/applied-mutation.json")))
            policies.current()
            store.write(RecoveryEnvelopeV1.parse(fixture("valid/new-baseline.json")))
            assertEquals(RecoveryNeeded, policies.current())

            store.clear()
            assertTrue(policies.current() is RecoveryNeeded)
        } finally {
            directory.deleteRecursively()
        }
    }
}
