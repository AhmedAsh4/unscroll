package org.unscroll.launcher

import android.os.Bundle
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.AdaptiveIconDrawable
import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNotNull
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.After
import org.junit.Assume.assumeFalse
import java.security.MessageDigest
import org.unscroll.launcher.catalog.IconStream
import org.unscroll.launcher.bridge.BridgeError
import org.unscroll.launcher.bridge.UnscrollProvider

class BridgeProviderInstrumentationTest {
    private var wroteFixture = false
    private val uri = android.net.Uri.parse("content://org.unscroll.launcher.bridge")
    private fun request(operation: String, arguments: String = "{}") = """{"protocol_version":"bridge-v1","operation":"$operation","arguments":$arguments}"""
    private val automation get() = InstrumentationRegistry.getInstrumentation().uiAutomation
    private fun shell(command: String): ByteArray = automation.executeShellCommand(command).let { descriptor ->
        ParcelFileDescriptor.AutoCloseInputStream(descriptor).use { it.readBytes() }
    }
    private fun call(operation: String, arguments: String = "{}"): String = shell("content call --uri content://org.unscroll.launcher.bridge --method bridge-v1 --extra string request '${request(operation, arguments)}'").decodeToString()
    private fun success(response: String) = assertTrue(response, response.contains("\"ok\":true"))
    private fun field(response: String, name: String): String = Regex("\\\"$name\\\":\\\"([^\\\"]+)\\\"").find(response)?.groupValues?.get(1) ?: error("missing $name: $response")
    private fun number(response: String, name: String): Int = Regex("\\\"$name\\\":([0-9]+)").find(response)?.groupValues?.get(1)?.toInt() ?: error("missing $name: $response")
    private fun envelope(response: String): String {
        val encoded = Regex("\\\"envelope\\\":\\\"((?:\\\\\\\\.|[^\\\"])*)\\\"").find(response)?.groupValues?.get(1) ?: error("missing envelope: $response")
        return encoded.replace(Regex("\\\\(.)")) { match -> when (match.groupValues[1]) { "n" -> "\n"; "r" -> "\r"; "t" -> "\t"; else -> match.groupValues[1] } }
    }

    @After fun removeTestFixture() {
        if (wroteFixture) shell("run-as ${InstrumentationRegistry.getInstrumentation().targetContext.packageName} rm -f files/recovery-v1.json")
    }

    @Test fun ordinaryTestApkReadAndWriteAreDeniedWithTheDocumentedForbiddenContract() {
        val ordinary = InstrumentationRegistry.getInstrumentation().context.contentResolver
        listOf(request("read_envelope", """{"device_binding":{}}"""), request("write_envelope", """{"device_binding":{},"envelope":"{}"}""")).forEach { frame ->
            try { ordinary.call(uri, "bridge-v1", null, Bundle().apply { putString("request", frame) }); fail("separate test UID must be denied") } catch (_: SecurityException) { }
        }
        val provider = UnscrollProvider()
        listOf("read_envelope", "write_envelope").forEach { operation ->
            val response = provider.call("bridge-v1", null, Bundle().apply { putString("request", request(operation)) }).getString("response")
            assertTrue(response, response?.contains("\"code\":\"${BridgeError.FORBIDDEN.code}\"") == true)
        }
    }

    @Test fun adbShellUsesBridgeAndReadsBoundedIconPipe() {
        success(call("health"))
        val serial = shell("getprop ro.serialno").decodeToString().trim()
        assumeTrue("target serial must be non-generic", serial.isNotBlank() && !serial.contains("generic", true) && !serial.contains("sdk", true))
        val facts = call("device_facts", """{"serial":"$serial"}""")
        success(facts)
        val fingerprint = field(facts, "fingerprint")
        val binding = """{"serial":"$serial","fingerprint":"$fingerprint","user_id":0}"""
        val oversizedIdentifier = "a." + "b".repeat(511)
        success(call("catalog_page", """{"cursor":null,"page_size":1,"device_binding":$binding}"""))
        assertTrue(call("catalog_page", """{"cursor":null,"page_size":1,"device_binding":{"serial":"$serial","fingerprint":"wrong","user_id":0}}""").contains("device_mismatch"))
        assertTrue(call("icon_stream", """{"package_id":"$oversizedIdentifier","activity_name":"org.unscroll.launcher.MainActivity","user_id":0}""").contains("invalid_request"))
        assertTrue(call("icon_stream", """{"package_id":"org.unscroll.launcher","activity_name":"$oversizedIdentifier","user_id":0}""").contains("invalid_request"))
        val icon = call("icon_stream", """{"package_id":"${InstrumentationRegistry.getInstrumentation().targetContext.packageName}","activity_name":"org.unscroll.launcher.MainActivity","user_id":0}""")
        success(icon)
        val stream = field(icon, "stream_id")
        val length = number(icon, "byte_length")
        val hash = field(icon, "sha256")
        val bytes = shell("content read --uri content://org.unscroll.launcher.bridge/bridge-v1/icon/$stream")
        assertEquals(length, bytes.size)
        assertEquals(hash, MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) })
        assertNotNull(BitmapFactory.decodeByteArray(bytes, 0, bytes.size))
        assertTrue(bytes.size <= 1_048_576)
        assertTrue(call("read_envelope", """{"device_binding":{"serial":"$serial","fingerprint":"$fingerprint","user_id":0}}""").contains("not_found"))
        assertTrue(call("write_envelope", """{"device_binding":{"serial":"$serial","fingerprint":"$fingerprint","user_id":0},"envelope":"{}"}""").contains("corrupt_envelope"))
    }

    @Test fun shellWritesAndReadsAnIsolatedValidRecoveryEnvelope() {
        val serial = shell("getprop ro.serialno").decodeToString().trim()
        assumeTrue("target serial must be non-generic", serial.isNotBlank() && !serial.contains("generic", true) && !serial.contains("sdk", true))
        val facts = call("device_facts", """{"serial":"$serial"}""")
        success(facts)
        val fingerprint = field(facts, "fingerprint")
        val binding = """{"serial":"$serial","fingerprint":"$fingerprint","user_id":0}"""
        assumeTrue("test must not replace an existing recovery envelope", call("read_envelope", """{"device_binding":$binding}""").contains("not_found"))
        val home = shell("cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME").decodeToString().lineSequence().firstOrNull { '/' in it } ?: error("unresolved HOME")
        val packageId = home.substringBefore('/')
        val unchecked = FIXTURE.replace("google/pixel/test", fingerprint).replace("ABC123", serial).replace("com.android.launcher3/.Launcher", home).replace("com.android.launcher3", packageId)
        val checksum = MessageDigest.getInstance("SHA-256").digest(unchecked.replace(Regex(",\"checksum\":\"[0-9a-f]{64}\""), "").toByteArray()).joinToString("") { "%02x".format(it) }
        val envelope = unchecked.replace(Regex("\"checksum\":\"[0-9a-f]{64}\""), "\"checksum\":\"$checksum\"")
        success(call("write_envelope", """{"device_binding":$binding,"envelope":${json(envelope)}}"""))
        wroteFixture = true
        val read = call("read_envelope", """{"device_binding":$binding}""")
        success(read)
        val returned = envelope(read)
        assertTrue(returned.contains("\"checksum\":\"$checksum\""))
        assertTrue(returned.contains("\"revision\":0"))
    }

    @Test fun generatedAndAdaptiveIconsEncodeAsBoundedPortablePng() {
        val drawables = buildList {
            add(ColorDrawable(Color.MAGENTA))
            if (Build.VERSION.SDK_INT >= 26) add(AdaptiveIconDrawable(ColorDrawable(Color.BLACK), ColorDrawable(Color.WHITE)))
        }
        drawables.forEach { drawable ->
            val png = IconStream.fromDrawable(drawable).bytes
            val bitmap = BitmapFactory.decodeByteArray(png, 0, png.size)
            assertNotNull(bitmap)
            assertTrue(bitmap.width <= 512 && bitmap.height <= 512)
            assertTrue(png.size in 1..1_048_576)
            assertTrue(png.copyOfRange(0, 8).contentEquals(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)))
        }
    }

    private fun json(value: String) = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

    private companion object {
        const val FIXTURE = """{"active_policy":{"allowed_packages":["com.phone"],"baseline_launcher_package":"com.android.launcher3"},"baseline":{"baseline_id":"11111111-1111-1111-1111-111111111111","baseline_launcher_package":"com.android.launcher3","initial_home_component":"com.android.launcher3/.Launcher","initial_packages":[{"package_id":"com.phone","suspended":false,"user_id":0}]},"baseline_id":"11111111-1111-1111-1111-111111111111","checksum":"bfaa54451e67cc9cf3fa537fe33c6cc6dedac12aeff6cf2b356a12402171706f","device_binding":{"fingerprint":"google/pixel/test","serial":"ABC123","user_id":0},"journal":[],"maintenance":{"state":"closed"},"previous_revision_hash":null,"revision":0,"schema_version":"recovery-v1"}"""
    }
}
