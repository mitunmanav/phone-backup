package org.phonebackup.companion

import android.Manifest
import android.app.Activity
import android.os.Bundle
import android.widget.TextView

/** The PC opens this for the on-phone step: permissions. */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(TextView(this).apply {
            text = "Phone Backup Helper\n\nFollow the steps on your computer."
            setTextAppearance(android.R.style.TextAppearance_DeviceDefault_Medium)
            val margin = (24 * resources.displayMetrics.density).toInt()
            // Android 15 and later draw under the status bar unless the screen makes room for it.
            setOnApplyWindowInsetsListener { view, insets ->
                @Suppress("DEPRECATION")
                view.setPadding(margin, margin + insets.systemWindowInsetTop, margin, margin)
                insets
            }
        })
        requestPermissions(arrayOf(Manifest.permission.READ_CONTACTS, Manifest.permission.READ_SMS, Manifest.permission.READ_CALL_LOG, Manifest.permission.READ_CALENDAR), 1)
    }
}
