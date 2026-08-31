package org.unscroll.launcher

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LauncherIntentTest {
    @Test
    fun `home and launcher intents reset the launcher to home`() {
        assertTrue(isLauncherEntryIntent("android.intent.action.MAIN", setOf("android.intent.category.HOME")))
        assertTrue(isLauncherEntryIntent("android.intent.action.MAIN", setOf("android.intent.category.LAUNCHER")))
        assertFalse(isLauncherEntryIntent("android.intent.action.VIEW", emptySet()))
    }
}
