package org.unscroll.launcher

import android.content.ComponentName
import android.content.Intent
import android.content.SharedPreferences
import android.provider.Settings
import android.provider.Telephony
import android.telecom.TelecomManager
import android.content.ActivityNotFoundException
import android.os.Bundle
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import org.unscroll.launcher.catalog.LauncherEntry
import org.unscroll.launcher.catalog.visibleEntries
import org.unscroll.launcher.policy.ActivePolicyStore
import org.unscroll.launcher.policy.PolicyFilter
import org.unscroll.launcher.recovery.PrivateEnvelopeStore

class MainActivity : AppCompatActivity() {
    private lateinit var prefs: SharedPreferences
    private lateinit var policyStore: ActivePolicyStore
    private lateinit var content: LinearLayout
    private var entries: List<LauncherEntry> = emptyList()
    private var drawerOpen = false
    private var downY = 0f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = getSharedPreferences("unscroll-launcher", MODE_PRIVATE)
        policyStore = ActivePolicyStore(PrivateEnvelopeStore(filesDir))
        entries = packageManager.queryIntentActivities(
            Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER),
            0,
        ).map { resolveInfo ->
            LauncherEntry(
                resolveInfo.loadLabel(packageManager).toString(),
                resolveInfo.activityInfo.packageName,
                resolveInfo.activityInfo.name,
            )
        }.sortedBy { it.label.lowercase() }
        showHome()
    }

    override fun onBackPressed() {
        if (drawerOpen) showHome()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (isLauncherEntryIntent(intent.action, intent.categories.orEmpty())) showHome()
    }

    private fun showHome() {
        drawerOpen = false
        showScreen(getString(R.string.app_name), visibleEntries(policyEntries(), packageName, "").take(8), false)
    }

    private fun showDrawer(query: String = "") {
        drawerOpen = true
        showScreen(getString(R.string.all_apps), visibleEntries(policyEntries(), packageName, query), true, query)
    }

    private fun showScreen(title: String, listed: List<LauncherEntry>, searchable: Boolean, query: String = "") {
        val appList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(32), dp(24), dp(24))
            setOnTouchListener { _, event ->
                when (event.action) {
                    MotionEvent.ACTION_DOWN -> downY = event.y
                    MotionEvent.ACTION_UP -> if (!drawerOpen && downY - event.y > dp(48)) showDrawer()
                }
                true
            }
        }
        content.addView(TextView(this).apply {
            text = title
            textSize = textSize()
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, 0, 0, dp(16))
        })
        if (searchable) {
            content.addView(EditText(this).apply {
                hint = getString(R.string.search)
                setSingleLine()
                setText(query)
                setSelection(query.length)
                addTextChangedListener(SimpleTextWatcher { renderEntries(appList, visibleEntries(policyEntries(), packageName, it)) })
            })
        }
        content.addView(ScrollView(this).apply {
            addView(appList)
        }, LinearLayout.LayoutParams(-1, 0, 1f))
        renderEntries(appList, listed)
        if (!searchable) {
            content.addView(Button(this).apply { text = getString(R.string.all_apps); setOnClickListener { showDrawer() } })
            content.addView(Button(this).apply {
                text = getString(R.string.text_size)
                setOnClickListener { prefs.edit().putFloat("text_size", if (textSize() == 20f) 24f else 20f).apply(); showHome() }
            })
        }
        setContentView(content)
    }

    private fun appButton(entry: LauncherEntry): View = Button(this).apply {
        text = entry.label
        textSize = textSize()
        gravity = Gravity.START or Gravity.CENTER_VERTICAL
        setOnClickListener {
            try {
                startActivity(Intent().setComponent(ComponentName(entry.packageName, entry.activityName)))
            } catch (_: ActivityNotFoundException) {
                Toast.makeText(this@MainActivity, R.string.unable_to_launch_app, Toast.LENGTH_SHORT).show()
            }
        }
    }

    private fun renderEntries(container: LinearLayout, listed: List<LauncherEntry>) {
        container.removeAllViews()
        listed.forEach { container.addView(appButton(it)) }
    }

    private fun policyEntries(): List<LauncherEntry> = PolicyFilter.filter(
        entries,
        policyStore.current(),
        packageName,
        protectedPackages(),
    )

    private fun protectedPackages(): Set<String> = PolicyFilter.protectedPackages(
        packageName,
        runCatching { packageManager.resolveActivity(Intent(Settings.ACTION_SETTINGS), 0)?.activityInfo?.packageName }.getOrNull(),
        runCatching { Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)?.substringBefore('/') }.getOrNull(),
        runCatching { getSystemService(TelecomManager::class.java)?.defaultDialerPackage }.getOrNull(),
        runCatching { Telephony.Sms.getDefaultSmsPackage(this) }.getOrNull(),
    )

    private fun textSize(): Float = prefs.getFloat("text_size", 20f)
    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()
}

fun isLauncherEntryIntent(action: String?, categories: Set<String>): Boolean =
    action == Intent.ACTION_MAIN && (Intent.CATEGORY_HOME in categories || Intent.CATEGORY_LAUNCHER in categories)
