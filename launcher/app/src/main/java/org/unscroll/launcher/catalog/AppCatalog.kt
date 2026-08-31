package org.unscroll.launcher.catalog

data class LauncherEntry(
    val label: String,
    val packageName: String,
    val activityName: String,
)

fun visibleEntries(entries: List<LauncherEntry>, ownPackage: String, query: String): List<LauncherEntry> {
    val needle = query.trim()
    return entries.filter { it.packageName != ownPackage && (needle.isEmpty() || it.label.contains(needle, true)) }
}
