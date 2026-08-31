package org.unscroll.launcher

import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ManifestCapabilitiesTest {
    @Test
    fun manifestOmitsRestrictedCapabilities() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val info = context.packageManager.getPackageInfo(
            context.packageName,
            PackageManager.GET_PERMISSIONS or PackageManager.GET_SERVICES or PackageManager.GET_RECEIVERS,
        )
        val permissions = info.requestedPermissions.orEmpty().toSet()

        assertFalse(permissions.contains("android.permission.INTERNET"))
        assertFalse(permissions.contains("android.permission.PACKAGE_USAGE_STATS"))
        assertFalse(permissions.contains("android.permission.BIND_ACCESSIBILITY_SERVICE"))
        assertFalse(permissions.contains("android.permission.BIND_DEVICE_ADMIN"))
        assertTrue(info.services.isNullOrEmpty())
        assertTrue(info.receivers.isNullOrEmpty())
    }

    @Test
    fun homeAndLauncherIntentsResolveAndLaunchMainActivity() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val component = ComponentName(context, MainActivity::class.java)

        listOf(Intent.CATEGORY_HOME, Intent.CATEGORY_LAUNCHER).forEach { category ->
            val intent = Intent(Intent.ACTION_MAIN)
                .addCategory(category)
                .setPackage(context.packageName)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            assertTrue(
                context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
                    .any { it.activityInfo.packageName == component.packageName && it.activityInfo.name == component.className },
            )

            ActivityScenario.launch<MainActivity>(intent).use { scenario ->
                assertEquals(Lifecycle.State.RESUMED, scenario.state)
            }
        }
    }

    @Test
    fun backOnHomeKeepsLauncherVisible() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val homeIntent = Intent(Intent.ACTION_MAIN)
            .addCategory(Intent.CATEGORY_HOME)
            .setComponent(ComponentName(context, MainActivity::class.java))

        ActivityScenario.launch<MainActivity>(homeIntent).use { scenario ->
            scenario.onActivity { it.onBackPressed() }
            assertEquals(Lifecycle.State.RESUMED, scenario.state)
        }
    }
}
