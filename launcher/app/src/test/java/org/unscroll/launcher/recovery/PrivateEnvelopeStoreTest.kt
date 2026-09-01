package org.unscroll.launcher.recovery

import java.io.File
import java.io.IOException
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test

class PrivateEnvelopeStoreTest {
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
    fun `writes and reads the complete validated envelope`() {
        val directory = Files.createTempDirectory("envelope-store").toFile()
        try {
            val envelope = RecoveryEnvelopeV1.parse(fixture("valid/applied-mutation.json"))
            val store = PrivateEnvelopeStore(directory)

            store.write(envelope)

            assertEquals(envelope.canonicalJson(), store.read()?.canonicalJson())
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun `interrupted write leaves the previously readable envelope intact`() {
        val directory = Files.createTempDirectory("envelope-store").toFile()
        try {
            val previous = RecoveryEnvelopeV1.parse(fixture("valid/new-baseline.json"))
            val replacement = RecoveryEnvelopeV1.parse(fixture("valid/applied-mutation.json"))
            PrivateEnvelopeStore(directory).write(previous)
            val interrupted = PrivateEnvelopeStore(directory, beforeCommit = { throw IOException("interrupted") })

            try {
                interrupted.write(replacement)
                fail("interrupted write should fail")
            } catch (_: IOException) {
            }

            assertEquals(previous.canonicalJson(), PrivateEnvelopeStore(directory).read()?.canonicalJson())
        } finally {
            directory.deleteRecursively()
        }
    }

    @Test
    fun `missing corrupt and cleared files have no envelope`() {
        val directory = Files.createTempDirectory("envelope-store").toFile()
        try {
            val store = PrivateEnvelopeStore(directory)
            assertNull(store.read())

            File(directory, "recovery-v1.json").writeText("not json")
            assertNull(store.read())

            store.clear()
            assertNull(store.read())
        } finally {
            directory.deleteRecursively()
        }
    }
}
