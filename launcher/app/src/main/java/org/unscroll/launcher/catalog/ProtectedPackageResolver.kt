package org.unscroll.launcher.catalog

import android.content.Context
import android.content.Intent
import android.content.res.Resources
import android.provider.Settings
import android.provider.Telephony
import android.telecom.TelecomManager
import android.os.Process
import android.net.Uri
import android.view.inputmethod.InputMethodManager

data class ProtectedRoleFacts(
    val systemUi: String? = null, val settings: String? = null, val permissionInfrastructure: String? = null,
    val packageInfrastructure: String? = null, val inputMethod: String? = null, val dialer: String? = null,
    val emergency: String? = null, val sms: String? = null, val provisioning: String? = null,
    val account: String? = null, val uncertain: Set<String> = emptySet(),
    val permissionCandidates: Set<String> = emptySet(), val packageCandidates: Set<String> = emptySet(),
    val imeCandidates: Set<String> = emptySet(), val smsCandidates: Set<String> = emptySet(),
    val emergencyCandidates: Set<String> = emptySet(), val provisioningCandidates: Set<String> = emptySet(), val accountCandidates: Set<String> = emptySet(),
)

object ProtectedPackageResolver {
    fun resolve(facts: ProtectedRoleFacts, launcherPackage: String, baselineLauncherPackage: String): Map<String, String> = buildMap {
        fun add(value: String?, reason: String) { if (!value.isNullOrBlank()) put(value, reason) }
        add(facts.systemUi, "System UI"); add(facts.settings, "Settings"); add(facts.permissionInfrastructure, "permission infrastructure")
        add(facts.packageInfrastructure, "package infrastructure"); add(facts.inputMethod, "active input method"); add(facts.dialer, "default dialer")
        add(facts.emergency, "emergency calling"); add(facts.sms, "default SMS app"); add(facts.provisioning, "device provisioning")
        add(facts.account, "account dependency"); add(launcherPackage, "Unscroll Launcher"); add(baselineLauncherPackage, "baseline launcher")
        facts.permissionCandidates.forEach { add(it, "permission infrastructure (unresolved or ambiguous)") }
        facts.packageCandidates.forEach { add(it, "package infrastructure (unresolved or ambiguous)") }
        facts.imeCandidates.forEach { add(it, "input method candidate (unresolved or ambiguous)") }
        facts.smsCandidates.forEach { add(it, "SMS candidate (unresolved or ambiguous)") }
        facts.emergencyCandidates.forEach { add(it, "emergency calling candidate (unresolved or ambiguous)") }
        facts.provisioningCandidates.forEach { add(it, "provisioning candidate (unresolved or ambiguous)") }
        facts.accountCandidates.forEach { add(it, "account candidate (unresolved or ambiguous)") }
        facts.uncertain.forEach { add(it, "manufacturer or shared-role dependency") }
    }

    fun resolve(context: Context, launcherPackage: String, baselineLauncherPackage: String): Map<String, String> {
        val pm = context.packageManager
        fun activities(intent: Intent) = runCatching { pm.queryIntentActivities(intent, 0).map { it.activityInfo.packageName }.toSet() }.getOrDefault(emptySet())
        val systemUi = runCatching {
            val id = Resources.getSystem().getIdentifier("config_systemUIServiceComponent", "string", "android")
            if (id == 0) null else Resources.getSystem().getString(id).substringBefore('/')
        }.getOrNull()
        val permission = runCatching { pm.queryIntentServices(Intent("android.permission.PermissionControllerService"), 0).map { it.serviceInfo.packageName }.toSet() }.getOrDefault(emptySet()).ifEmpty { pm.getInstalledPackages(0).map { it.packageName }.toSet() }
        val permissionController = permission.singleOrNull()
        val packages = activities(Intent(Intent.ACTION_INSTALL_PACKAGE)) + activities(Intent(Intent.ACTION_UNINSTALL_PACKAGE))
        val emergency = activities(Intent("android.intent.action.EMERGENCY_DIAL")) + activities(Intent(Intent.ACTION_DIAL))
        val provisioning = activities(Intent("android.app.action.PROVISION_MANAGED_DEVICE"))
        val accounts = activities(Intent(Settings.ACTION_ADD_ACCOUNT))
        val sms = activities(Intent(Intent.ACTION_SENDTO, Uri.parse("smsto:")))
        val imes = context.getSystemService(InputMethodManager::class.java)?.inputMethodList.orEmpty().map { it.packageName }.toSet()
        return resolve(ProtectedRoleFacts(
            systemUi, activities(Intent(Settings.ACTION_SETTINGS)).firstOrNull(), permissionController,
            activities(Intent(Intent.ACTION_INSTALL_PACKAGE)).firstOrNull(), Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)?.substringBefore('/'),
            context.getSystemService(TelecomManager::class.java)?.defaultDialerPackage, activities(Intent("android.intent.action.EMERGENCY_DIAL")).firstOrNull(),
            Telephony.Sms.getDefaultSmsPackage(context), activities(Intent("android.app.action.PROVISION_MANAGED_DEVICE")).firstOrNull(), activities(Intent(Settings.ACTION_ADD_ACCOUNT)).firstOrNull(),
            uncertain = buildSet {
                addAll(pm.getPackagesForUid(Process.SYSTEM_UID).orEmpty())
                listOf(Intent(Settings.ACTION_SETTINGS), Intent(Intent.ACTION_INSTALL_PACKAGE), Intent(Intent.ACTION_UNINSTALL_PACKAGE), Intent("android.intent.action.EMERGENCY_DIAL"), Intent(Intent.ACTION_DIAL), Intent("android.app.action.PROVISION_MANAGED_DEVICE"), Intent(Settings.ACTION_ADD_ACCOUNT)).forEach { addAll(activities(it)) }
            }, permissionCandidates = permission, packageCandidates = packages, imeCandidates = imes, smsCandidates = sms,
            emergencyCandidates = emergency, provisioningCandidates = provisioning, accountCandidates = accounts,
        ), launcherPackage, baselineLauncherPackage)
    }
}
