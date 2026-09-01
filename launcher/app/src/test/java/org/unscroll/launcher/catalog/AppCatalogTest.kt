package org.unscroll.launcher.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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

    @Test
    fun `catalog keeps launchable enabled apps in stable label package order`() {
        val catalog = AppCatalog.create(
            listOf(
                CatalogCandidate("com.zebra", "Same", enabled = true, suspended = false, launchable = true, hasIcon = true),
                CatalogCandidate("com.alpha", "Same", enabled = true, suspended = false, launchable = true, hasIcon = false),
                CatalogCandidate("com.disabled", "Disabled", enabled = false, suspended = false, launchable = true, hasIcon = true),
                CatalogCandidate("com.hidden", "Hidden", enabled = true, suspended = false, launchable = false, hasIcon = true),
            ),
            protected = mapOf("com.zebra" to "default dialer"),
        )

        assertEquals(
            listOf("com.disabled", "com.alpha", "com.zebra"),
            catalog.entries.map { it.packageId },
        )
        assertEquals(null, catalog.entries.first().protectedReason)
        assertEquals("default dialer", catalog.entries.last().protectedReason)
        assertFalse(catalog.entries.first { it.packageId == "com.alpha" }.iconAvailable)
    }

    @Test
    fun `catalog paging is stable and bounded`() {
        val catalog = AppCatalog.create(
            (1..3).map { CatalogCandidate("com.example.app$it", "App $it", true, false, true, true) },
            emptyMap(),
        )

        assertEquals(listOf("com.example.app1", "com.example.app2"), catalog.page(0, 2).entries.map { it.packageId })
        assertEquals(listOf("com.example.app3"), catalog.page(2, 100).entries.map { it.packageId })
    }

    @Test
    fun `catalog exposes the chosen launcher activity and real icon resources`() {
        val entry = AppCatalog.create(
            listOf(
                CatalogCandidate(
                    packageId = "com.example.camera",
                    activityName = "com.example.camera.CameraActivity",
                    label = "Camera",
                    enabled = true,
                    suspended = false,
                    launchable = true,
                    hasActivityIcon = true,
                    hasApplicationIcon = true,
                ),
            ),
            emptyMap(),
        ).entries.single()

        assertEquals("com.example.camera.CameraActivity", entry.activityName)
        assertTrue(entry.activityIconAvailable)
        assertTrue(entry.applicationIconAvailable)
        assertTrue(entry.iconAvailable)
    }
}
