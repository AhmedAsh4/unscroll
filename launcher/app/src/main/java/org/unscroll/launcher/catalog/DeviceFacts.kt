package org.unscroll.launcher.catalog

import android.os.Build
import org.unscroll.launcher.recovery.RecoveryDeviceBinding

data class BridgeCapabilities(val appOps: Boolean = false, val homeSelection: Boolean = false, val packageSuspension: Boolean = false, val recoveryStorage: Boolean = true)
data class DeviceFacts(val binding: RecoveryDeviceBinding?, val capabilities: BridgeCapabilities = BridgeCapabilities())
object DeviceFactsProvider {
    fun current(serial: String?): DeviceFacts = DeviceFacts(trustedBinding(serial, Build.FINGERPRINT))

    fun trustedBinding(serial: String?, fingerprint: String?): RecoveryDeviceBinding? {
        fun trusted(value: String?) = value?.takeIf { it.isNotBlank() && it.length <= 512 && it.all { char -> char.code in 0x20..0x7e } && !it.lowercase().let { text -> text == "unknown" || text == "generic" || "generic" in text || "sdk" in text } }
        return trusted(serial)?.let { safeSerial -> trusted(fingerprint)?.let { RecoveryDeviceBinding(safeSerial, it, 0) } }
    }
}
