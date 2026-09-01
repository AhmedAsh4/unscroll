package org.unscroll.launcher.bridge

import android.content.ComponentName
import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.content.pm.PackageManager
import android.os.Process
import android.os.Binder
import android.os.UserHandle
import org.unscroll.launcher.catalog.AppCatalog
import org.unscroll.launcher.catalog.DeviceFactsProvider
import org.unscroll.launcher.catalog.IconStream
import org.unscroll.launcher.catalog.ProtectedPackageResolver
import org.unscroll.launcher.recovery.PrivateEnvelopeStore
import org.unscroll.launcher.recovery.RecoveryEnvelopeV1
import org.unscroll.launcher.recovery.RecoveryValidationException
import java.io.FileOutputStream
import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

class UnscrollProvider : ContentProvider() {
    private data class Snapshot(val entries: List<org.unscroll.launcher.catalog.CatalogEntry>, val offset: Int, val expiresAt: Long)
    private data class Stream(val bytes: ByteArray, val expiresAt: Long)
    private val pages = linkedMapOf<String, Snapshot>()
    private val streams = linkedMapOf<String, Stream>()
    private val lock = Any()
    private val pipeExecutor = ThreadPoolExecutor(1, 2, 30, TimeUnit.SECONDS, ArrayBlockingQueue(2), ThreadPoolExecutor.AbortPolicy())

    override fun onCreate() = true
    override fun call(method: String, arg: String?, extras: Bundle?): Bundle = Bundle().apply {
        val response = try {
            if (!isPrimaryUser() || Binder.getCallingUid() != Process.SHELL_UID) throw BridgeException(BridgeError.FORBIDDEN)
            if (context?.checkCallingPermission("android.permission.DUMP") != PackageManager.PERMISSION_GRANTED) throw BridgeException(BridgeError.FORBIDDEN)
            if (method != BridgeProtocol.VERSION) throw BridgeException(BridgeError.UNSUPPORTED)
            if (arg != null || extras?.keySet() != setOf("request")) throw BridgeException(BridgeError.INVALID_REQUEST)
            val request = extras.getString("request") ?: throw BridgeException(BridgeError.INVALID_REQUEST)
            handle(BridgeProtocol.parseRequest(request))
        } catch (error: BridgeException) { BridgeProtocol.failure(error.error) }
        catch (_: Exception) { BridgeProtocol.failure(BridgeError.INTERNAL) }
        putString("response", response)
    }

    private fun handle(request: BridgeRequest): String {
        val app = context ?: throw BridgeException(BridgeError.INTERNAL)
        return when (request.operation) {
            "health" -> { BridgeProtocol.objectArgs(request, emptySet()); BridgeProtocol.response(mapOf("protocol_version" to BridgeProtocol.VERSION, "recovery_schema" to "recovery-v1", "launcher_package" to app.packageName)) }
            "device_facts" -> { val serial = BridgeProtocol.string(BridgeProtocol.objectArgs(request, setOf("serial"))["serial"]); val facts = DeviceFactsProvider.current(serial); val binding = facts.binding ?: throw BridgeException(BridgeError.DEVICE_MISMATCH); BridgeProtocol.response(mapOf("device_binding" to mapOf("serial" to binding.serial, "fingerprint" to binding.fingerprint, "user_id" to binding.userId), "capabilities" to mapOf("app_ops" to facts.capabilities.appOps, "home_selection" to facts.capabilities.homeSelection, "package_suspension" to facts.capabilities.packageSuspension, "recovery_storage" to facts.capabilities.recoveryStorage))) }
            "catalog_page" -> catalog(app, request, 0)
            "icon_stream" -> icon(app, request, 0)
            "read_envelope" -> envelope(app, request, false)
            "write_envelope" -> envelope(app, request, true)
            else -> throw BridgeException(BridgeError.UNSUPPORTED)
        }
    }

    private fun catalog(app: android.content.Context, request: BridgeRequest, userId: Long): String {
        val args = BridgeProtocol.objectArgs(request, setOf("cursor", "page_size")); val size = BridgeProtocol.number(args["page_size"])
        if (size !in 1..100) throw BridgeException(BridgeError.INVALID_REQUEST)
        val previous = args["cursor"]
        val snapshot = if (previous == null) {
            val storeFile = java.io.File(app.filesDir, "recovery-v1.json")
            val baseline = PrivateEnvelopeStore(app.filesDir).read()?.baselineLauncherPackage
            val home = app.packageManager.resolveActivity(android.content.Intent(android.content.Intent.ACTION_MAIN).addCategory(android.content.Intent.CATEGORY_HOME), 0)?.activityInfo?.packageName
            val protected = if (baseline != null) ProtectedPackageResolver.resolve(app, app.packageName, baseline) else if (!storeFile.exists() && home != null && home != app.packageName) ProtectedPackageResolver.resolve(app, app.packageName, home) else AppCatalog.installed(app.packageManager, emptyMap(), userId).entries.associate { it.packageId to "recovery baseline unavailable" }
            val entries = AppCatalog.installed(app.packageManager, protected, userId).entries
            if (entries.size > MAX_CATALOG_ENTRIES) throw BridgeException(BridgeError.INTERNAL)
            Snapshot(entries, 0, System.currentTimeMillis() + TTL)
        } else synchronized(lock) { cleanup(); pages.remove(BridgeProtocol.string(previous).also { if (!it.matches(Regex("[0-9a-f]{32}"))) throw BridgeException(BridgeError.INVALID_REQUEST) }) } ?: throw BridgeException(BridgeError.NOT_FOUND)
        val batch = snapshot.entries.drop(snapshot.offset).take(size.toInt()); val nextOffset = snapshot.offset + batch.size
        val next = if (nextOffset < snapshot.entries.size) token().also { synchronized(lock) { cleanup(); if (pages.size >= MAX_PAGES) throw BridgeException(BridgeError.INTERNAL); pages[it] = snapshot.copy(offset = nextOffset) } } else null
        return BridgeProtocol.response(mapOf("entries" to batch.map { mapOf("package_id" to it.packageId, "activity_name" to it.activityName, "label" to it.label, "user_id" to it.userId, "launchable" to it.launchable, "enabled" to it.enabled, "suspended" to it.suspended, "activity_icon_available" to it.activityIconAvailable, "application_icon_available" to it.applicationIconAvailable, "icon_available" to it.iconAvailable, "protected_reason" to it.protectedReason) }, "next_cursor" to next))
    }

    private fun icon(app: android.content.Context, request: BridgeRequest, userId: Long): String {
        val args = BridgeProtocol.objectArgs(request, setOf("package_id", "activity_name", "user_id")); val packageId = BridgeProtocol.string(args["package_id"]); val activityName = BridgeProtocol.string(args["activity_name"]); if (BridgeProtocol.number(args["user_id"]) != userId || !packageId.matches(Regex("[A-Za-z_][A-Za-z0-9_$]*(\\.[A-Za-z_][A-Za-z0-9_$]*)+")) || !activityName.matches(Regex("[A-Za-z_][A-Za-z0-9_$]*(\\.[A-Za-z_][A-Za-z0-9_$]*)+"))) throw BridgeException(BridgeError.INVALID_REQUEST)
        val component = ComponentName(packageId, activityName)
        val launchable = app.packageManager.queryIntentActivities(android.content.Intent(android.content.Intent.ACTION_MAIN).addCategory(android.content.Intent.CATEGORY_LAUNCHER).setPackage(packageId), PackageManager.MATCH_DISABLED_COMPONENTS).any { it.activityInfo.name == activityName }
        if (!launchable) throw BridgeException(BridgeError.NOT_FOUND)
        val payload = try { IconStream.fromActivity(app.packageManager, component) } catch (_: Exception) { throw BridgeException(BridgeError.NOT_FOUND) }
        val id = token(); synchronized(lock) { cleanup(); if (streams.size >= MAX_STREAMS || streams.values.sumOf { it.bytes.size } + payload.bytes.size > MAX_STREAM_BYTES) throw BridgeException(BridgeError.INTERNAL); streams[id] = Stream(payload.bytes, System.currentTimeMillis() + TTL) }
        return BridgeProtocol.response(mapOf("byte_length" to payload.bytes.size.toLong(), "mime_type" to payload.mimeType, "sha256" to payload.sha256, "stream_id" to id))
    }

    private fun envelope(app: android.content.Context, request: BridgeRequest, write: Boolean): String {
        val args = BridgeProtocol.objectArgs(request, if (write) setOf("device_binding", "envelope") else setOf("device_binding")); val supplied = args["device_binding"] as? Map<*, *> ?: throw BridgeException(BridgeError.INVALID_REQUEST); val binding = DeviceFactsProvider.current(supplied["serial"] as? String).binding ?: throw BridgeException(BridgeError.DEVICE_MISMATCH); if (supplied != mapOf("serial" to binding.serial, "fingerprint" to binding.fingerprint, "user_id" to binding.userId)) throw BridgeException(BridgeError.DEVICE_MISMATCH)
        val store = PrivateEnvelopeStore(app.filesDir, binding)
        val storeFile = java.io.File(app.filesDir, "recovery-v1.json")
        val existing = store.read()
        if (!write) return BridgeProtocol.response(mapOf("envelope" to (existing?.canonicalJson() ?: throw BridgeException(if (storeFile.exists()) BridgeError.CONFLICT else BridgeError.NOT_FOUND))))
        val submitted = BridgeProtocol.string(args["envelope"])
        val next = try { RecoveryEnvelopeV1.parseForDevice(submitted, binding.serial, binding.fingerprint, binding.userId) } catch (_: RecoveryValidationException) { throw BridgeException(BridgeError.CORRUPT_ENVELOPE) }
        if (submitted != next.canonicalJson()) throw BridgeException(BridgeError.CORRUPT_ENVELOPE)
        try {
            if (existing == null) {
                val home = app.packageManager.resolveActivity(android.content.Intent(android.content.Intent.ACTION_MAIN).addCategory(android.content.Intent.CATEGORY_HOME), 0)?.activityInfo?.packageName
                if (storeFile.exists() || home == null || home == app.packageName || next.baselineLauncherPackage != home) throw BridgeException(BridgeError.CONFLICT)
            }
            store.writeIfExtends(next)
        } catch (_: RecoveryValidationException) { throw BridgeException(BridgeError.CONFLICT) } catch (_: java.io.IOException) { throw BridgeException(BridgeError.CONFLICT) }
        return BridgeProtocol.response(mapOf("checksum" to next.checksum, "revision" to next.revision))
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (!isPrimaryUser() || Binder.getCallingUid() != Process.SHELL_UID || context?.checkCallingPermission("android.permission.DUMP") != PackageManager.PERMISSION_GRANTED || mode != "r" || uri.query != null || uri.fragment != null || uri.pathSegments.size != 3 || uri.pathSegments[0] != BridgeProtocol.VERSION || uri.pathSegments[1] != "icon") throw java.io.FileNotFoundException()
        val bytes = synchronized(lock) { cleanup(); streams.remove(uri.pathSegments[2])?.bytes } ?: throw java.io.FileNotFoundException()
        val pipe = ParcelFileDescriptor.createPipe(); try { pipeExecutor.execute { pipe[1].use { descriptor -> FileOutputStream(descriptor.fileDescriptor).use { it.write(bytes) } } } } catch (_: Exception) { pipe[0].close(); pipe[1].close(); throw java.io.FileNotFoundException() }; return pipe[0]
    }
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? = null
    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0
    private fun token(): String = ByteArray(16).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it) }
    private fun isPrimaryUser(): Boolean = Process.myUserHandle() == UserHandle.getUserHandleForUid(0)
    private fun cleanup() { val now = System.currentTimeMillis(); pages.entries.removeIf { it.value.expiresAt < now }; streams.entries.removeIf { it.value.expiresAt < now } }
    private companion object { const val MAX_PAGES = 32; const val MAX_STREAMS = 8; const val MAX_STREAM_BYTES = 4 * IconStream.MAX_BYTES; const val MAX_CATALOG_ENTRIES = 1_000; const val TTL = 300_000L }
}
