package org.unscroll.launcher.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgeProtocolTest {
    @Test
    fun `accepts only the versioned closed request frame`() {
        val request = BridgeProtocol.parseRequest("""{"protocol_version":"bridge-v1","operation":"health","arguments":{}}""")

        assertEquals("health", request.operation)
        assertTrue(request.arguments.isEmpty())
    }

    @Test
    fun `rejects unknown fields and oversized requests`() {
        assertEquals(
            BridgeError.INVALID_REQUEST,
            BridgeProtocol.parseError("""{"protocol_version":"bridge-v1","operation":"health","arguments":{},"extra":true}"""),
        )
        assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError("x".repeat(65_537)))
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
    fun `accepts max envelope inside bounded outer frame and rejects oversized envelope`() {
        fun request(size: Int) = """{"protocol_version":"bridge-v1","operation":"write_envelope","arguments":{"device_binding":{},"envelope":"${"x".repeat(size)}"}}"""
        assertEquals("write_envelope", BridgeProtocol.parseRequest(request(65_536)).operation)
        assertEquals(BridgeError.INVALID_REQUEST, BridgeProtocol.parseError(request(65_537)))
    }
}
