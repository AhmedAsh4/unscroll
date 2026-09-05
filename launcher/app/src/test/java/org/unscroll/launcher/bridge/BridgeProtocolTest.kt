package org.unscroll.launcher.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class BridgeProtocolTest {
    @Test
    fun `accepts only the versioned closed request frame`() {
        val request = BridgeProtocol.parseRequest("""{"protocol_version":"bridge-v1","operation":"health","arguments":{}}""")

        assertEquals("health", request.operation)
        assertTrue(request.arguments.isEmpty())
    }

    @Test
    fun `rejects unknown fields and oversized frames`() {
        assertEquals(
            BridgeError.INVALID_REQUEST,
            BridgeProtocol.parseError("""{"protocol_version":"bridge-v1","operation":"health","arguments":{},"extra":true}"""),
        )
        assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError("x".repeat(262_145)))
    }

    @Test
    fun `rejects duplicate null keys deep input malformed unicode and long page sizes`() {
        listOf(
            """{"protocol_version":"bridge-v1","operation":"health","arguments":null,"arguments":{}}""",
            "[".repeat(33) + "]".repeat(33),
            "\"\\uZZZZ\"",
            "\"\\ud800\"",
            "\"\uD800\"",
            """{"protocol_version":"bridge-v1","operation":"catalog_page","arguments":{"cursor":null,"page_size":999999999999999999999}}""",
        ).forEach { assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError(it)) }
    }

    @Test
    fun `accepts a bounded recovery envelope payload larger than narrow strings`() {
        val request = """{"protocol_version":"bridge-v1","operation":"write_envelope","arguments":{"device_binding":{"serial":"ABC123","fingerprint":"google/pixel/test","user_id":0},"envelope":"${"x".repeat(693)}"}}"""

        assertEquals("write_envelope", BridgeProtocol.parseRequest(request).operation)
        assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError(request.replace("x".repeat(693), "x".repeat(65_537))))
    }

    @Test
    fun `accepts the worst escaped max envelope inside bounded request and response frames`() {
        fun quote(value: String) = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""
        fun request(envelope: String) = """{"protocol_version":"bridge-v1","operation":"write_envelope","arguments":{"device_binding":{},"envelope":${quote(envelope)}}}"""
        val maxEnvelope = "\"".repeat(65_536)
        val request = request(maxEnvelope)

        assertTrue(request.toByteArray().size <= 262_144)
        assertEquals(maxEnvelope, BridgeProtocol.parseRequest(request).arguments["envelope"])
        assertTrue(BridgeProtocol.response(mapOf("envelope" to maxEnvelope)).toByteArray().size <= 262_144)
        assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError(request("\"".repeat(65_537))))
    }

    @Test
    fun `rejects catalog and icon metadata above 512 bytes`() {
        assertEquals("x".repeat(512), BridgeProtocol.boundedString("x".repeat(512)))
        try {
            BridgeProtocol.boundedString("x".repeat(513))
            fail("metadata above the bridge bound must reject")
        } catch (error: BridgeException) {
            assertEquals(BridgeError.INVALID_REQUEST, error.error)
        }
    }
}
