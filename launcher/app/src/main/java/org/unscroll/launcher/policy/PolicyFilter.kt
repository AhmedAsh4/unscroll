package org.unscroll.launcher.policy

import org.unscroll.launcher.catalog.LauncherEntry

object PolicyFilter {
    fun filter(
        entries: List<LauncherEntry>,
        state: LauncherPolicyState,
        ownPackage: String,
        protectedPackages: Set<String>,
    ): List<LauncherEntry> = (state as? ActivePolicy)?.let { policy ->
        filter(entries, policy, ownPackage, protectedPackages)
    }.orEmpty()

    fun filter(
        entries: List<LauncherEntry>,
        policy: ActivePolicy,
        ownPackage: String,
        protectedPackages: Set<String>,
    ): List<LauncherEntry> = entries.filter { entry ->
        entry.packageName in policy.allowedPackages &&
            entry.packageName != ownPackage &&
            entry.packageName != policy.baselineLauncherPackage &&
            entry.packageName !in protectedPackages
    }

}
