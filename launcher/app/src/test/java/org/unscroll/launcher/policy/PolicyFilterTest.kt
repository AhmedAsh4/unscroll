package org.unscroll.launcher.policy

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.unscroll.launcher.catalog.LauncherEntry
import org.unscroll.launcher.catalog.visibleEntries

class PolicyFilterTest {
    private val entries = listOf(
        LauncherEntry("Phone", "com.phone", "PhoneActivity"),
        LauncherEntry("Social", "com.social", "SocialActivity"),
        LauncherEntry("Unscroll Launcher", "org.unscroll.launcher", "MainActivity"),
        LauncherEntry("Baseline", "com.android.launcher3", "LauncherActivity"),
        LauncherEntry("Settings", "com.android.settings", "SettingsActivity"),
    )

    private val policy = ActivePolicy(
        allowedPackages = setOf("com.phone", "com.android.launcher3", "com.android.settings", "org.unscroll.launcher"),
        baselineLauncherPackage = "com.android.launcher3",
        revision = 2,
    )

    @Test
    fun `renders only installed allowed packages excluding launcher baseline and protected packages`() {
        assertEquals(
            listOf(LauncherEntry("Phone", "com.phone", "PhoneActivity")),
            PolicyFilter.filter(entries, policy, "org.unscroll.launcher", setOf("com.android.settings")),
        )
    }

    @Test
    fun `never infers removed or newly installed apps from the allowlist`() {
        val removed = PolicyFilter.filter(entries.filterNot { it.packageName == "com.phone" }, policy, "org.unscroll.launcher", emptySet())
        val newlyInstalled = PolicyFilter.filter(
            entries + LauncherEntry("New app", "com.new", "NewActivity"),
            policy,
            "org.unscroll.launcher",
            emptySet(),
        )

        assertTrue(removed.none { it.packageName == "com.phone" })
        assertTrue(newlyInstalled.none { it.packageName == "com.new" })
    }

    @Test
    fun `search remains label-only and case-insensitive after policy filtering`() {
        val visible = PolicyFilter.filter(entries, policy, "org.unscroll.launcher", emptySet())

        assertEquals(listOf(LauncherEntry("Phone", "com.phone", "PhoneActivity")), visibleEntries(visible, "org.unscroll.launcher", "pHo"))
        assertTrue(visibleEntries(visible, "org.unscroll.launcher", "com.phone").isEmpty())
    }

    @Test
    fun `recovery-needed state cannot expose an inferred launcher list`() {
        assertTrue(PolicyFilter.filter(entries, RecoveryNeeded, "org.unscroll.launcher", emptySet()).isEmpty())
    }
}
