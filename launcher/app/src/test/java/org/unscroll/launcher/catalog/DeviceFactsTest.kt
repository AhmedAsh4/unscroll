package org.unscroll.launcher.catalog

import org.junit.Assert.assertNull
import org.junit.Assert.assertEquals
import org.junit.Test

class DeviceFactsTest {
    @Test
    fun `unknown generic or blank serial never creates a recovery binding`() {
        assertNull(DeviceFactsProvider.trustedBinding("unknown", "google/pixel/test"))
        assertNull(DeviceFactsProvider.trustedBinding("generic_x86", "google/pixel/test"))
        assertNull(DeviceFactsProvider.trustedBinding("sdk_gphone", "google/pixel/test"))
        assertNull(DeviceFactsProvider.trustedBinding("", "google/pixel/test"))
        assertNull(DeviceFactsProvider.trustedBinding("ABC123", "unknown"))
        assertEquals("ABC123", DeviceFactsProvider.trustedBinding("ABC123", "google/pixel/test")?.serial)
    }
}
