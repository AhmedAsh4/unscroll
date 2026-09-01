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

    fun read(): RecoveryEnvelopeV1? = synchronized(LOCK) { readUnsafe() }
    private fun readUnsafe(): RecoveryEnvelopeV1? = (stateUnsafe() as? ReadState.Valid)?.envelope
    private fun stateUnsafe(): ReadState {
        if (!envelopeFile.exists()) return ReadState.Missing
        if (!envelopeFile.isFile) return ReadState.Unreadable
        val envelope = try { RecoveryEnvelopeV1.parse(envelopeFile.readText()) } catch (_: Exception) { return ReadState.Unreadable }
        return if (expectedBinding == null || expectedBinding == envelope.deviceBinding) ReadState.Valid(envelope) else ReadState.Unreadable
    }

    @Throws(IOException::class)
    fun write(envelope: RecoveryEnvelopeV1) = synchronized(LOCK) { writeUnsafe(envelope) }
    fun writeIfExtends(envelope: RecoveryEnvelopeV1) = synchronized(LOCK) {
        when (val current = stateUnsafe()) {
            is ReadState.Valid -> current.envelope.requireStrictPrefixOf(envelope)
            ReadState.Missing -> Unit
            ReadState.Unreadable -> throw IOException("unreadable private recovery envelope")
        }
        writeUnsafe(envelope)
    }
    private fun writeUnsafe(envelope: RecoveryEnvelopeV1) {
        if (!directory.isDirectory && !directory.mkdirs()) throw IOException("cannot create private recovery storage")
        if (expectedBinding != null && expectedBinding != envelope.deviceBinding) throw IOException("unexpected recovery device binding")
        if ((readUnsafe()?.revision ?: Long.MIN_VALUE) > envelope.revision) {
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
        val LOCK = Any()
    }

    private sealed class ReadState {
        data class Valid(val envelope: RecoveryEnvelopeV1) : ReadState()
        object Missing : ReadState()
        object Unreadable : ReadState()
    }
}
