package org.unscroll.launcher.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer

class IconStreamTest {
    @Test
    fun `bounded png payload has the portable png signature`() {
        val payload = IconStream.validatedPng(ONE_PIXEL_PNG)

        assertEquals("image/png", payload.mimeType)
        assertEquals(ONE_PIXEL_PNG.size, payload.bytes.size)
        assertTrue(payload.bytes.copyOfRange(0, 8).contentEquals(byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)))
    }

    @Test
    fun `activity icon is selected before an available application icon`() {
        assertEquals(IconSource.ACTIVITY, IconStream.selectSource(activityIconAvailable = true, applicationIconAvailable = true))
        assertEquals(IconSource.APPLICATION, IconStream.selectSource(activityIconAvailable = false, applicationIconAvailable = true))
        assertEquals(null, IconStream.selectSource(activityIconAvailable = false, applicationIconAvailable = false))
    }

    @Test
    fun `png payload carries 1x1 dimensions in its IHDR chunk`() {
        // javax.imageio is desktop-JVM-only and absent from the Android
        // unit-test classpath, so dimensions are read straight from IHDR:
        // width is the big-endian int at offset 16, height at offset 20.
        assertEquals(1, ByteBuffer.wrap(ONE_PIXEL_PNG).getInt(16))
        assertEquals(1, ByteBuffer.wrap(ONE_PIXEL_PNG).getInt(20))
    }

    companion object {
        private val ONE_PIXEL_PNG = java.util.Base64.getDecoder().decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR42mNk+M/wHwAF/gL+DK22nQAAAABJRU5ErkJggg==",
        )
    }
}
