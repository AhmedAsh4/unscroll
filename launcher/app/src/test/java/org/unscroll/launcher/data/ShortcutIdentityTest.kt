package org.unscroll.launcher.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class ShortcutIdentityTest {
    @Test
    fun `identity is stable and field boundaries cannot collide`() {
        assertEquals(
            shortcutIdentity("org.browser", "https://example.com", "UserHandle{0}"),
            shortcutIdentity("org.browser", "https://example.com", "UserHandle{0}"),
        )
        assertNotEquals(shortcutIdentity("ab", "c", "d"), shortcutIdentity("a", "bc", "d"))
    }
}
