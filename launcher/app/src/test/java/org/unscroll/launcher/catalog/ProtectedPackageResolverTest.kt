package org.unscroll.launcher.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProtectedPackageResolverTest {
    @Test
    fun `roles stay protected when resolved and baseline is permanent`() {
        val protected = ProtectedPackageResolver.resolve(
            ProtectedRoleFacts(
                systemUi = "com.android.systemui",
                settings = "com.android.settings",
                inputMethod = "com.android.inputmethod",
                dialer = "com.android.dialer",
                sms = "com.android.messaging",
                provisioning = "com.android.provision",
                account = "com.android.settings",
            ),
            launcherPackage = "org.unscroll.launcher",
            baselineLauncherPackage = "com.android.launcher3",
        )

        assertEquals("baseline launcher", protected["com.android.launcher3"])
        assertEquals("active input method", protected["com.android.inputmethod"])
        assertTrue("com.android.systemui" in protected)
    }

    @Test
    fun `unresolved telephony roles are absent while uncertain packages stay protected`() {
        val protected = ProtectedPackageResolver.resolve(
            ProtectedRoleFacts(uncertain = setOf("vendor.role.helper")),
            "org.unscroll.launcher",
            "com.android.launcher3",
        )

        assertEquals("manufacturer or shared-role dependency", protected["vendor.role.helper"])
        assertTrue("com.android.dialer" !in protected)
    }

    @Test
    fun `ambiguous safety roles retain every candidate with a reason`() {
        val protected = ProtectedPackageResolver.resolve(
            ProtectedRoleFacts(
                permissionCandidates = setOf("permission.one", "permission.two"),
                packageCandidates = setOf("package.one"),
                imeCandidates = setOf("ime.one"), smsCandidates = setOf("sms.one"),
                emergencyCandidates = setOf("emergency.one"), provisioningCandidates = setOf("provision.one"), accountCandidates = setOf("account.one"),
            ), "org.unscroll.launcher", "baseline.launcher",
        )

        assertEquals("permission infrastructure (unresolved or ambiguous)", protected["permission.one"])
        assertTrue(listOf("permission.two", "package.one", "ime.one", "sms.one", "emergency.one", "provision.one", "account.one").all { it in protected })
    }

    @Test
    fun `permission controller fallback candidates cannot disappear`() {
        val protected = ProtectedPackageResolver.resolve(
            ProtectedRoleFacts(permissionCandidates = setOf("catalog.one", "catalog.two")),
            "org.unscroll.launcher", "baseline.launcher",
        )

        assertTrue(setOf("catalog.one", "catalog.two").all { protected[it]?.contains("permission infrastructure") == true })
    }
}
