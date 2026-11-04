package org.phonebackup.companion

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.provider.CallLog
import android.provider.ContactsContract
import android.provider.Telephony
import java.io.BufferedWriter
import java.io.IOException
import java.io.Reader
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.MessageDigest
import org.json.JSONException
import org.json.JSONObject
import kotlin.concurrent.thread

private const val PORT = 47621
private const val PROTO = 1
private const val IDLE_MS = 10 * 60 * 1000
private const val MAX_LINE_CHARS = 64 * 1024

/** Answers the PC over a loopback socket that adb forwards. See docs/android-design.md section 3. */
class BridgeService : Service() {
    @Volatile private var token: String? = null
    private var serving = false

    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(1, notification())
        token = intent?.getStringExtra("token")
        if (token.isNullOrEmpty()) {
            stopSelf()
        } else if (!serving) {
            serving = true
            thread { serve() }
        }
        return START_NOT_STICKY
    }

    private fun notification(): Notification {
        val builder = if (Build.VERSION.SDK_INT >= 26) {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel("bridge", "Connected to computer", NotificationManager.IMPORTANCE_LOW),
            )
            Notification.Builder(this, "bridge")
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setContentTitle("Phone Backup is connected to your computer")
            .build()
    }

    private fun serve() {
        try {
            ServerSocket(PORT, 1, InetAddress.getByName("127.0.0.1")).use { server ->
                // Nothing else stops the service if the PC disappears.
                server.soTimeout = IDLE_MS
                var open = true
                while (open) open = server.accept().use { handle(it) }
            }
        } catch (_: IOException) {
            // A timeout or a dropped connection: the PC starts the service again if it still needs it.
        } finally {
            stopSelf()
        }
    }

    /** Runs one connection. Returns false once the PC says bye. */
    private fun handle(socket: Socket): Boolean {
        socket.soTimeout = IDLE_MS
        val reader = socket.getInputStream().bufferedReader()
        val writer = socket.getOutputStream().bufferedWriter()
        val first = parse(reader.readLimitedLine()) ?: return true
        val id = first.opt("id")
        if (first.optString("op") != "hello" || !tokenMatches(first.optString("token"))) {
            writer.send(JSONObject().put("id", id).put("ok", false).put("error", "bad_token"))
            return true
        }
        writer.send(hello(id))
        while (true) {
            val request = parse(reader.readLimitedLine()) ?: return true
            if (request.optString("op") == "bye") {
                writer.send(JSONObject().put("id", request.opt("id")).put("ok", true))
                return false
            }
            answer(request, writer)
        }
    }

    /** Null at the end of the stream or past the limit, so another app on the phone cannot exhaust memory. */
    private fun Reader.readLimitedLine(): String? {
        val line = StringBuilder()
        while (line.length <= MAX_LINE_CHARS) {
            val char = read()
            if (char == '\n'.code) return line.toString()
            if (char < 0) return null
            line.append(char.toChar())
        }
        return null
    }

    private fun parse(line: String?): JSONObject? = try {
        line?.let { JSONObject(it) }
    } catch (_: JSONException) {
        null
    }

    private fun tokenMatches(sent: String): Boolean {
        val expected = token ?: return false
        return MessageDigest.isEqual(sent.toByteArray(), expected.toByteArray())
    }

    private fun hello(id: Any?) = JSONObject()
        .put("id", id)
        .put("ok", true)
        .put("app", BuildConfigVersion)
        .put("proto", PROTO)
        .put("sdk", Build.VERSION.SDK_INT)
        .put("perms", JSONObject()
            .put("contacts", granted(Manifest.permission.READ_CONTACTS))
            .put("sms", granted(Manifest.permission.READ_SMS))
            .put("calllog", granted(Manifest.permission.READ_CALL_LOG)))

    private fun granted(permission: String) = checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

    private fun answer(request: JSONObject, writer: BufferedWriter) {
        val id = request.opt("id")
        try {
            when (request.optString("op")) {
                "list" -> when (request.optString("kind")) {
                    "contacts" -> list(
                        id,
                        ContactsContract.Data.CONTENT_URI,
                        arrayOf("contact_id", "raw_contact_id", "mimetype", "display_name", "is_primary", "is_super_primary") +
                            (1..15).map { "data$it" },
                        writer,
                    )
                    "sms" -> list(
                        id,
                        Telephony.Sms.CONTENT_URI,
                        arrayOf("address", "date", "date_sent", "read", "seen", "type", "status", "subject", "body", "locked"),
                        writer,
                    )
                    "calllog" -> list(
                        id,
                        CallLog.Calls.CONTENT_URI,
                        arrayOf("number", "name", "date", "duration", "type", "new", "is_read", "presentation", "features"),
                        writer,
                    )
                    else -> writer.send(error(id, "unknown_kind"))
                }
                else -> writer.send(error(id, "unknown_op"))
            }
        } catch (_: SecurityException) {
            writer.send(error(id, "permission_denied"))
        }
    }

    private fun error(id: Any?, code: String) = JSONObject().put("id", id).put("ok", false).put("error", code)

    private fun list(id: Any?, uri: Uri, columns: Array<String>, writer: BufferedWriter) {
        var count = 0
        contentResolver.query(uri, columns, null, null, null)?.use { rows ->
            while (rows.moveToNext()) {
                val item = JSONObject()
                for (column in 0 until rows.columnCount) {
                    // shortcut: photos are left out because bytes do not travel in JSON, add them with `adb pull` (section 3.3) when restore needs them.
                    if (rows.getType(column) !in listOf(Cursor.FIELD_TYPE_NULL, Cursor.FIELD_TYPE_BLOB)) {
                        item.put(rows.getColumnName(column), rows.getString(column))
                    }
                }
                writer.send(JSONObject().put("id", id).put("item", item))
                count++
            }
        }
        writer.send(JSONObject().put("id", id).put("end", true).put("count", count))
    }

    private val BuildConfigVersion: String
        get() = packageManager.getPackageInfo(packageName, 0).versionName ?: ""

    private fun BufferedWriter.send(message: JSONObject) {
        write(message.toString())
        newLine()
        flush()
    }
}
