package org.unscroll.launcher.shortcut

import android.content.pm.LauncherApps
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import org.unscroll.launcher.R

class PinItemActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!supportsPinShortcuts(Build.VERSION.SDK_INT)) {
            Toast.makeText(this, R.string.pin_action_not_supported, Toast.LENGTH_SHORT).show()
            finish()
            return
        }
        handleRequest()
    }

    @RequiresApi(Build.VERSION_CODES.O)
    private fun handleRequest() {
        val request = getSystemService(LauncherApps::class.java).getPinItemRequest(intent)
        val message = when {
            request == null -> R.string.invalid_pin_request
            request.requestType != LauncherApps.PinItemRequest.REQUEST_TYPE_SHORTCUT -> R.string.pin_action_not_supported
            request.shortcutInfo == null -> R.string.invalid_shortcut
            runCatching { request.accept() }.getOrDefault(false) -> R.string.shortcut_pinned
            else -> R.string.shortcut_pin_failed
        }
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
        finish()
    }
}

fun supportsPinShortcuts(apiLevel: Int): Boolean = apiLevel >= Build.VERSION_CODES.O
