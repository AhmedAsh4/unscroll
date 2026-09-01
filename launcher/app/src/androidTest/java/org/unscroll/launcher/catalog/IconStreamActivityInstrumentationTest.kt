package org.unscroll.launcher.catalog

import android.content.ComponentName
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.graphics.Color
import android.os.Build
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Test

class IconStreamActivityInstrumentationTest {
    @Test
    fun activityIconWinsOverApplicationIcon() {
        val payload = IconStream.fromActivity(context.packageManager, COMPONENT)
        val bitmap = BitmapFactory.decodeByteArray(payload.bytes, 0, payload.bytes.size)

        assertNotNull(bitmap)
        assertEquals(Color.RED, bitmap!!.getPixel(bitmap.width / 2, bitmap.height / 2))
    }

    @Test
    fun missingActivityAndApplicationIconsAreRejected() {
        val testContext = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().context
        val component = ComponentName(testContext, MissingIconActivity::class.java)
        val info = testContext.packageManager.getActivityInfo(component, PackageManager.MATCH_DISABLED_COMPONENTS)
        assertEquals(0, info.icon)
        assertEquals(0, info.applicationInfo.icon)
        try {
            IconStream.fromActivity(testContext.packageManager, component)
            fail("missing resources must not become the Android generic icon")
        } catch (_: android.content.pm.PackageManager.NameNotFoundException) {
        }
    }

    @Test
    fun adaptiveActivityIconProducesBoundedDecodablePng() {
        assumeTrue(Build.VERSION.SDK_INT >= 26)
        val payload = IconStream.fromActivity(context.packageManager, COMPONENT)
        val bitmap = BitmapFactory.decodeByteArray(payload.bytes, 0, payload.bytes.size)

        assertNotNull(bitmap)
        assertTrue(bitmap!!.width <= 512 && bitmap.height <= 512)
        assertTrue(payload.bytes.size in 1..IconStream.MAX_BYTES)
    }

    private val context get() = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext

    private companion object {
        val COMPONENT = ComponentName("org.unscroll.launcher.debug", "org.unscroll.launcher.testicon.IconFixtureActivity")
    }
}
