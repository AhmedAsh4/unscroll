package org.unscroll.launcher.catalog

import org.junit.Assert.assertEquals
import org.junit.Test

class AppCatalogTest {
    @Test
    fun `search returns only matching text entries and excludes this launcher`() {
        val entries = listOf(
            LauncherEntry("Camera", "com.android.camera", "CameraActivity"),
            LauncherEntry("Messages", "com.android.messages", "MessagesActivity"),
            LauncherEntry("Unscroll Launcher", "org.unscroll.launcher", "MainActivity"),
        )

        assertEquals(
            listOf(LauncherEntry("Camera", "com.android.camera", "CameraActivity")),
            visibleEntries(entries, "org.unscroll.launcher", "cam"),
        )
    }
}
