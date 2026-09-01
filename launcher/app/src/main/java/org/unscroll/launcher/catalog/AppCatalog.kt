package org.unscroll.launcher.catalog

import android.content.Intent
import android.content.pm.PackageManager
import java.util.Locale

data class LauncherEntry(val label: String, val packageName: String, val activityName: String)
data class CatalogCandidate(val packageId: String, val label: String, val enabled: Boolean, val suspended: Boolean, val launchable: Boolean, val hasIcon: Boolean = false, val activityName: String = "", val hasActivityIcon: Boolean = hasIcon, val hasApplicationIcon: Boolean = hasIcon)
data class CatalogEntry(val packageId: String, val activityName: String, val label: String, val userId: Long, val launchable: Boolean, val enabled: Boolean, val suspended: Boolean, val activityIconAvailable: Boolean, val applicationIconAvailable: Boolean, val iconAvailable: Boolean, val protectedReason: String?)
data class CatalogPage(val entries: List<CatalogEntry>)
data class AppCatalog(val entries: List<CatalogEntry>) {
    fun page(offset: Int, size: Int): CatalogPage = CatalogPage(entries.drop(offset.coerceAtLeast(0)).take(size.coerceIn(1, MAX_PAGE)))

    companion object {
        const val MAX_PAGE = 100

        fun create(candidates: List<CatalogCandidate>, protected: Map<String, String>, userId: Long = 0): AppCatalog = AppCatalog(
            candidates.asSequence().filter { it.launchable }.groupBy { it.packageId }
                .map { (_, matches) -> matches.minWith(compareBy<CatalogCandidate> { it.label.lowercase(Locale.ROOT) }.thenBy { it.label }.thenBy { it.activityName }) }
                .sortedWith(compareBy<CatalogCandidate> { it.label.lowercase(Locale.ROOT) }.thenBy { it.packageId })
                .map { CatalogEntry(it.packageId, it.activityName, ascii(it.label, it.packageId), userId, true, it.enabled, it.suspended, it.hasActivityIcon, it.hasApplicationIcon, it.hasActivityIcon || it.hasApplicationIcon, protected[it.packageId]) }.toList(),
        )

        fun installed(packageManager: PackageManager, protected: Map<String, String>, userId: Long): AppCatalog = create(
            packageManager.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), PackageManager.MATCH_DISABLED_COMPONENTS).map { resolved ->
                val activity = resolved.activityInfo
                val app = activity.applicationInfo
                val activityIcon = IconStream.hasResource(packageManager, activity.packageName, activity.icon, app)
                val applicationIcon = IconStream.hasResource(packageManager, app.packageName, app.icon, app)
                CatalogCandidate(activity.packageName, resolved.loadLabel(packageManager).toString(), app.enabled && activity.enabled, (app.flags and android.content.pm.ApplicationInfo.FLAG_SUSPENDED) != 0, true, activityIcon || applicationIcon, activity.name, activityIcon, applicationIcon)
            },
            protected,
            userId,
        )
    }
}

fun visibleEntries(entries: List<LauncherEntry>, ownPackage: String, query: String): List<LauncherEntry> {
    val needle = query.trim()
    return entries.filter { it.packageName != ownPackage && (needle.isEmpty() || it.label.contains(needle, true)) }
}

internal fun ascii(value: String, fallback: String): String = value.filter { it.code in 0x20..0x7e }.take(512).ifBlank { fallback }
