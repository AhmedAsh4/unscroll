package org.unscroll.launcher.shortcut

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PinItemCompatibilityTest {
    @Test
    fun `pin shortcuts require Android O or later`() {
        assertFalse(supportsPinShortcuts(24))
        assertTrue(supportsPinShortcuts(26))
    }
}
