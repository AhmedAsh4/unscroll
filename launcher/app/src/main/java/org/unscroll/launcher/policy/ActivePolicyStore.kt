package org.unscroll.launcher.policy

import org.unscroll.launcher.recovery.PrivateEnvelopeStore

sealed interface LauncherPolicyState

data class ActivePolicy(
    val allowedPackages: Set<String>,
    val baselineLauncherPackage: String,
    val revision: Long,
) : LauncherPolicyState

data object RecoveryNeeded : LauncherPolicyState

class ActivePolicyStore(private val envelopes: PrivateEnvelopeStore) {
    private var newestRevision: Long? = null

    fun current(): LauncherPolicyState {
        val envelope = envelopes.read() ?: return RecoveryNeeded
        val revision = envelope.revision
        if (newestRevision?.let { revision < it } == true) return RecoveryNeeded
        newestRevision = revision
        return ActivePolicy(envelope.activeAllowedPackages, envelope.baselineLauncherPackage, revision)
    }
}
