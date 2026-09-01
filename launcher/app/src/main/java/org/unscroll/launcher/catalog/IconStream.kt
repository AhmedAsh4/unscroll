package org.unscroll.launcher.catalog

import android.content.ComponentName
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.graphics.drawable.Drawable
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

data class IconPayload(val bytes: ByteArray, val mimeType: String = "image/png") {
    val sha256: String = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
}

enum class IconSource { ACTIVITY, APPLICATION }

object IconStream {
    const val MAX_BYTES = 1_048_576
    private const val MAX_DIMENSION = 512
    private val PNG = byteArrayOf(-119, 80, 78, 71, 13, 10, 26, 10)
    fun selectSource(activityIconAvailable: Boolean, applicationIconAvailable: Boolean): IconSource? = when {
        activityIconAvailable -> IconSource.ACTIVITY
        applicationIconAvailable -> IconSource.APPLICATION
        else -> null
    }
    fun fromActivity(packageManager: PackageManager, component: ComponentName): IconPayload {
        val activity = packageManager.getActivityInfo(component, PackageManager.MATCH_DISABLED_COMPONENTS)
        val application = activity.applicationInfo
        val source = selectSource(hasResource(packageManager, activity.packageName, activity.icon, application), hasResource(packageManager, application.packageName, application.icon, application)) ?: throw PackageManager.NameNotFoundException(component.flattenToShortString())
        val drawable = when (source) {
            IconSource.ACTIVITY -> packageManager.getActivityIcon(component)
            IconSource.APPLICATION -> packageManager.getApplicationIcon(application)
        }
        return fromDrawable(drawable)
    }
    fun hasResource(packageManager: PackageManager, packageName: String, resource: Int, applicationInfo: ApplicationInfo): Boolean = resource != 0 && runCatching { packageManager.getDrawable(packageName, resource, applicationInfo) }.getOrNull() != null
    fun fromDrawable(drawable: Drawable): IconPayload {
        var size = MAX_DIMENSION
        while (size >= 1) {
            val width = drawable.intrinsicWidth.takeIf { it > 0 }?.coerceAtMost(size) ?: size
            val height = drawable.intrinsicHeight.takeIf { it > 0 }?.coerceAtMost(size) ?: size
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            val previous = Rect(drawable.bounds)
            drawable.setBounds(0, 0, width, height); Canvas(bitmap).also(drawable::draw); drawable.bounds = previous
            val bytes = ByteArrayOutputStream().use { output -> bitmap.compress(Bitmap.CompressFormat.PNG, 100, output); output.toByteArray() }
            bitmap.recycle()
            if (bytes.size in 1..MAX_BYTES) return IconPayload(bytes)
            size /= 2
        }
        throw IllegalArgumentException("icon exceeds bridge limit")
    }
    fun validatedPng(bytes: ByteArray): IconPayload {
        require(bytes.size in 1..MAX_BYTES && bytes.size >= PNG.size && bytes.copyOfRange(0, PNG.size).contentEquals(PNG)) { "invalid PNG" }
        return IconPayload(bytes)
    }
}
