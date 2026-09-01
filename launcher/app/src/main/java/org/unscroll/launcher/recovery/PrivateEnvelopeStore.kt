package org.unscroll.launcher.recovery

import java.io.File
import java.io.FileOutputStream
import java.io.IOException

class PrivateEnvelopeStore(
    private val directory: File,
    private val expectedBinding: RecoveryDeviceBinding? = null,
    private val beforeCommit: () -> Unit = {},
) {
    private val envelopeFile = File(directory, FILE_NAME)
    private val temporaryFile = File(directory, "$FILE_NAME.tmp")

    fun read(): RecoveryEnvelopeV1? = try {
        val envelope = envelopeFile.takeIf(File::isFile)?.readText()?.let(RecoveryEnvelopeV1::parse) ?: return null
        if (expectedBinding == null || expectedBinding == envelope.deviceBinding) envelope else null
    } catch (_: Exception) {
        null
    }

    @Throws(IOException::class)
    fun write(envelope: RecoveryEnvelopeV1) {
        if (!directory.isDirectory && !directory.mkdirs()) throw IOException("cannot create private recovery storage")
        if (expectedBinding != null && expectedBinding != envelope.deviceBinding) throw IOException("unexpected recovery device binding")
        if ((read()?.revision ?: Long.MIN_VALUE) > envelope.revision) {
            throw IOException("stale private recovery envelope")
        }
        try {
            FileOutputStream(temporaryFile).use { output ->
                output.write(envelope.canonicalJson().toByteArray(Charsets.UTF_8))
                output.fd.sync()
            }
            beforeCommit()
            if (!temporaryFile.renameTo(envelopeFile)) throw IOException("cannot replace private recovery envelope")
        } finally {
            temporaryFile.delete()
        }
    }

    fun clear() {
        envelopeFile.delete()
        temporaryFile.delete()
    }

    private companion object {
        const val FILE_NAME = "recovery-v1.json"
    }
}
