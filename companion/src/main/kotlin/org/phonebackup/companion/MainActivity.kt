package org.phonebackup.companion

import android.Manifest
import android.app.Activity
import android.app.role.RoleManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.provider.Telephony
import android.widget.TextView

/** The PC opens this for the on-phone steps: permissions, and making the helper the messages app for a restore. */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // A messages app must open for "send a message to"; the helper sends nothing.
        if (intent?.action == Intent.ACTION_SENDTO) return finish()
        val smsRole = intent?.getBooleanExtra("sms_role", false) == true
        setContentView(TextView(this).apply {
            text = if (smsRole) {
                "Phone Backup Helper\n\nTo put your messages back, choose Phone Backup Helper as the messages app for now. Your computer tells you how to switch back."
            } else {
                "Phone Backup Helper\n\nFollow the steps on your computer."
            }
            setTextAppearance(android.R.style.TextAppearance_DeviceDefault_Medium)
            val margin = (24 * resources.displayMetrics.density).toInt()
            // Android 15 and later draw under the status bar unless the screen makes room for it.
            setOnApplyWindowInsetsListener { view, insets ->
                @Suppress("DEPRECATION")
                view.setPadding(margin, margin + insets.systemWindowInsetTop, margin, margin)
                insets
            }
        })
        if (smsRole) {
            val ask = if (Build.VERSION.SDK_INT >= 29) {
                getSystemService(RoleManager::class.java).createRequestRoleIntent(RoleManager.ROLE_SMS)
            } else {
                Intent(Telephony.Sms.Intents.ACTION_CHANGE_DEFAULT).putExtra(Telephony.Sms.Intents.EXTRA_PACKAGE_NAME, packageName)
            }
            @Suppress("DEPRECATION")
            startActivityForResult(ask, 2)
            return
        }
        requestPermissions(
            arrayOf(
                Manifest.permission.READ_CONTACTS, Manifest.permission.READ_SMS, Manifest.permission.READ_CALL_LOG,
                Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CONTACTS, Manifest.permission.WRITE_CALL_LOG,
                Manifest.permission.WRITE_CALENDAR,
            ),
            1,
        )
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        // The PC sees the answer through sms_role.
        finish()
    }
}
