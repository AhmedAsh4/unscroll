package org.unscroll.launcher.recovery

import java.io.File
import java.io.FileOutputStream
import java.io.IOException

class PrivateEnvelopeStore(
    private val directory: File,
    private val beforeCommit: () -> Unit = {},
) {
    private val envelopeFile = File(directory, FILE_NAME)
    private val temporaryFile = File(directory, "$FILE_NAME.tmp")
    private val bindingFile = File(directory, BINDING_FILE_NAME)
    private val revisionFile = File(directory, REVISION_FILE_NAME)

    fun read(): RecoveryEnvelopeV1? = try {
        val envelope = envelopeFile.takeIf(File::isFile)?.readText()?.let(RecoveryEnvelopeV1::parse) ?: return null
        val binding = bindingFile.takeIf(File::isFile)?.readLines()?.takeIf { it.size == 3 } ?: return null
        val revision = revisionFile.takeIf(File::isFile)?.readText()?.toLongOrNull() ?: return null
        if (
            binding != listOf(envelope.deviceBinding.serial, envelope.deviceBinding.fingerprint, envelope.deviceBinding.userId.toString()) ||
            envelope.revision < revision
        ) null else envelope
    } catch (_: Exception) {
        null
    }

    @Throws(IOException::class)
    fun write(envelope: RecoveryEnvelopeV1) {
        if (!directory.isDirectory && !directory.mkdirs()) throw IOException("cannot create private recovery storage")
        try {
            FileOutputStream(temporaryFile).use { output ->
                output.write(envelope.canonicalJson().toByteArray(Charsets.UTF_8))
                output.fd.sync()
            }
            beforeCommit()
            if (!temporaryFile.renameTo(envelopeFile)) throw IOException("cannot replace private recovery envelope")
            bindingFile.writeText("${envelope.deviceBinding.serial}\n${envelope.deviceBinding.fingerprint}\n${envelope.deviceBinding.userId}")
            revisionFile.writeText(maxOf(revisionFile.takeIf(File::isFile)?.readText()?.toLongOrNull() ?: envelope.revision, envelope.revision).toString())
        } finally {
            temporaryFile.delete()
        }
    }

    fun clear() {
        envelopeFile.delete()
        temporaryFile.delete()
        bindingFile.delete()
        revisionFile.delete()
    }

    private companion object {
        const val FILE_NAME = "recovery-v1.json"
        const val BINDING_FILE_NAME = "recovery-v1.binding"
        const val REVISION_FILE_NAME = "recovery-v1.revision"
    }
}
