package org.phonebackup.companion

import android.app.Service
import android.content.BroadcastReceiver
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.provider.Telephony

// Android lets only the phone's messages app add messages, and only an app with these parts can
// become it. The helper holds that role for a few minutes during a restore; see
// docs/android-design.md section 3.4.

/** Keeps a text message that arrives while the helper is the messages app, so none is lost. */
class SmsReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val parts = Telephony.Sms.Intents.getMessagesFromIntent(intent)
        val first = parts?.firstOrNull() ?: return
        val sms = ContentValues().apply {
            put("address", first.displayOriginatingAddress)
            put("body", parts.joinToString("") { it.displayMessageBody ?: "" })
            put("date", System.currentTimeMillis())
            put("date_sent", first.timestampMillis)
            put("read", 0)
            put("seen", 0)
        }
        context.contentResolver.insert(Telephony.Sms.Inbox.CONTENT_URI, sms)
    }
}

/** Required of a messages app. A picture message arriving in those minutes is not kept. */
class MmsReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Unit
}

/** Required of a messages app: "reply with a message" to a call. The helper sends nothing. */
class RespondService : Service() {
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        stopSelf()
        return START_NOT_STICKY
    }
}
