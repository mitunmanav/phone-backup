package org.phonebackup.companion

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.ContentProviderOperation
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.provider.CalendarContract
import android.provider.CallLog
import android.provider.ContactsContract
import android.provider.Telephony
import java.io.BufferedWriter
import java.io.File
import java.io.IOException
import java.io.Reader
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.MessageDigest
import java.util.TimeZone
import org.json.JSONArray
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
        // Made by the app, so the PC's `adb push` of message attachments lands in a folder it can read.
        getExternalFilesDir("in")
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
            .put("calllog", granted(Manifest.permission.READ_CALL_LOG))
            .put("calendar", granted(Manifest.permission.READ_CALENDAR)))

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
                    "calendar" -> list(
                        id,
                        CalendarContract.Events.CONTENT_URI,
                        arrayOf(
                            "title", "dtstart", "dtend", "allDay", "eventLocation", "description", "rrule",
                            "duration", "eventTimezone", "eventEndTimezone", "rdate", "exrule", "exdate",
                            "availability", "accessLevel", "hasAlarm", "calendar_id", "calendar_displayName",
                            "account_name", "account_type",
                        ),
                        writer,
                    )
                    // Only what tells a restored picture message from one already on the phone.
                    "mms" -> list(id, Telephony.Mms.CONTENT_URI, arrayOf("date", "msg_box", "sub"), writer)
                    else -> writer.send(error(id, "unknown_kind"))
                }
                "insert" -> insert(id, request.optString("kind"), request.optJSONArray("items") ?: JSONArray(), writer)
                "sms_role" -> {
                    val current = Telephony.Sms.getDefaultSmsPackage(this)
                    writer.send(JSONObject()
                        .put("id", id)
                        .put("ok", true)
                        .put("default", current == packageName)
                        .put("package", current ?: "")
                        .put("name", current?.let { label(it) } ?: ""))
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

    /** Adds each item on its own, so one bad item fails alone. The PC sends only what the phone lacks. */
    private fun insert(id: Any?, kind: String, items: JSONArray, writer: BufferedWriter) {
        val add: (JSONObject) -> Boolean = when (kind) {
            "contacts" -> this::addContact
            "calllog" -> this::addCall
            "calendar" -> this::addEvent
            "sms" -> this::addSms
            "mms" -> this::addMms
            else -> return writer.send(error(id, "unknown_kind"))
        }
        var inserted = 0
        for (i in 0 until items.length()) {
            val ok = try {
                items.optJSONObject(i)?.let(add) ?: false
            } catch (_: Exception) {
                false
            }
            if (ok) inserted++
        }
        writer.send(JSONObject().put("id", id).put("ok", true).put("inserted", inserted).put("failed", items.length() - inserted))
    }

    /** The item's `keys` that it has, as text: the providers store them in their own types. */
    private fun values(item: JSONObject, vararg keys: String) = ContentValues().apply {
        for (key in keys) if (item.has(key) && !item.isNull(key)) put(key, item.optString(key))
    }

    /** One raw contact in the phone's own (local) account, with every detail the PC sends. */
    private fun addContact(item: JSONObject): Boolean {
        val rows = item.optJSONArray("rows") ?: return false
        val column = Regex("data[0-9]{1,2}|mimetype|is_primary|is_super_primary")
        val ops = arrayListOf(
            ContentProviderOperation.newInsert(ContactsContract.RawContacts.CONTENT_URI)
                .withValue(ContactsContract.RawContacts.ACCOUNT_TYPE, null)
                .withValue(ContactsContract.RawContacts.ACCOUNT_NAME, null)
                .build(),
        )
        for (i in 0 until rows.length()) {
            val row = rows.optJSONObject(i) ?: continue
            val op = ContentProviderOperation.newInsert(ContactsContract.Data.CONTENT_URI)
                .withValueBackReference(ContactsContract.Data.RAW_CONTACT_ID, 0)
            for (key in row.keys()) if (column.matches(key)) op.withValue(key, row.optString(key))
            ops += op.build()
        }
        contentResolver.applyBatch(ContactsContract.AUTHORITY, ops)
        return true
    }

    private fun addCall(item: JSONObject): Boolean {
        val call = values(item, "number", "name", "date", "duration", "type", "presentation", "features")
        // Old calls must not show up as new missed calls.
        call.put("new", 0)
        call.put("is_read", 1)
        return contentResolver.insert(CallLog.Calls.CONTENT_URI, call) != null
    }

    private var calendarId: Long? = null

    /** A local calendar of its own: Android lets no app write into a Google account's calendar. */
    private fun restoredCalendar(): Long {
        calendarId?.let { return it }
        val account = "Phone Backup"
        contentResolver.query(
            CalendarContract.Calendars.CONTENT_URI,
            arrayOf("_id"),
            "account_type=? AND account_name=?",
            arrayOf(CalendarContract.ACCOUNT_TYPE_LOCAL, account),
            null,
        )?.use { if (it.moveToFirst()) return it.getLong(0).also { found -> calendarId = found } }
        val uri = CalendarContract.Calendars.CONTENT_URI.buildUpon()
            .appendQueryParameter(CalendarContract.CALLER_IS_SYNCADAPTER, "true")
            .appendQueryParameter("account_name", account)
            .appendQueryParameter("account_type", CalendarContract.ACCOUNT_TYPE_LOCAL)
            .build()
        val calendar = ContentValues().apply {
            put("account_name", account)
            put("account_type", CalendarContract.ACCOUNT_TYPE_LOCAL)
            put("name", "phone_backup")
            put("calendar_displayName", "Restored by Phone Backup")
            put("calendar_color", 0xFF2E7D32.toInt())
            put("calendar_access_level", CalendarContract.Calendars.CAL_ACCESS_OWNER)
            put("ownerAccount", account)
            put("visible", 1)
            put("sync_events", 1)
            put("calendar_timezone", TimeZone.getDefault().id)
        }
        val id = contentResolver.insert(uri, calendar)?.lastPathSegment?.toLong() ?: error("no calendar")
        calendarId = id
        return id
    }

    private fun addEvent(item: JSONObject): Boolean {
        val event = values(
            item, "title", "dtstart", "dtend", "allDay", "eventLocation", "description", "rrule", "duration",
            "eventTimezone", "eventEndTimezone", "rdate", "exrule", "exdate", "availability", "accessLevel",
        )
        event.put("calendar_id", restoredCalendar())
        // Reminders are not in backups.
        event.put("hasAlarm", 0)
        val allDay = item.optString("allDay") == "1"
        // Android keeps all-day events in UTC.
        if (allDay) event.put("eventTimezone", "UTC")
        if (!event.containsKey("eventTimezone")) event.put("eventTimezone", TimeZone.getDefault().id)
        // A repeating event needs a duration and no end; any other needs an end.
        if (item.optString("rrule").isNotEmpty() || item.optString("rdate").isNotEmpty()) {
            if (item.optString("duration").isEmpty()) {
                val start = item.optString("dtstart").toLongOrNull() ?: return false
                val end = item.optString("dtend").toLongOrNull() ?: return false
                event.put("duration", if (allDay) "P${maxOf(1, (end - start) / 86_400_000)}D" else "P${(end - start) / 1000}S")
            }
            event.remove("dtend")
        } else {
            event.remove("duration")
        }
        return contentResolver.insert(CalendarContract.Events.CONTENT_URI, event) != null
    }

    /** Works only while the helper is the phone's messages app; the provider files it by number. */
    private fun addSms(item: JSONObject): Boolean {
        val sms = values(item, "address", "date", "date_sent", "read", "seen", "type", "status", "subject", "body", "locked")
        if (!sms.containsKey("read")) sms.put("read", 1)
        if (!sms.containsKey("seen")) sms.put("seen", 1)
        return contentResolver.insert(Telephony.Sms.CONTENT_URI, sms) != null
    }

    /** A picture message, its people and its parts. Attachments wait in files/in, put there by the PC. */
    private fun addMms(item: JSONObject): Boolean {
        val addresses = item.optJSONArray("addresses") ?: JSONArray()
        val parts = item.optJSONArray("parts") ?: JSONArray()
        val people = (0 until addresses.length())
            .mapNotNull { addresses.optJSONObject(it) }
            .filter { it.optString("address").let { a -> a.isNotEmpty() && a != "insert-address-token" } }
        val date = item.optString("date").toLongOrNull() ?: return false
        if (people.isEmpty()) return false
        val inbox = getExternalFilesDir("in") ?: return false
        val files = (0 until parts.length()).mapNotNull { parts.optJSONObject(it)?.optString("file") }.filter { it.isNotEmpty() }
        // A message must not come back without one of its pictures.
        if (files.any { !File(inbox, it).isFile }) return false
        val box = item.optString("msg_box").toIntOrNull() ?: 1
        val message = ContentValues().apply {
            put("thread_id", Telephony.Threads.getOrCreateThreadId(this@BridgeService, people.map { it.optString("address") }.toSet()))
            put("date", date)
            put("msg_box", box)
            put("read", 1)
            put("seen", 1)
            put("ct_t", "application/vnd.wap.multipart.related")
            // 132 is a received message, 128 a sent one.
            put("m_type", if (box == 1) 132 else 128)
            put("v", 18)
            put("text_only", if (files.isEmpty()) 1 else 0)
            item.optString("sub").takeIf { it.isNotEmpty() }?.let {
                put("sub", it)
                put("sub_cs", 106)
            }
        }
        val uri = contentResolver.insert(Telephony.Mms.CONTENT_URI, message) ?: return false
        val mid = uri.lastPathSegment ?: return false
        try {
            for (person in people) {
                contentResolver.insert(Uri.parse("content://mms/$mid/addr"), ContentValues().apply {
                    put("msg_id", mid)
                    put("address", person.optString("address"))
                    put("type", person.optString("type").toIntOrNull() ?: 151)
                    put("charset", 106)
                }) ?: error("no address")
            }
            for (i in 0 until parts.length()) {
                val part = parts.optJSONObject(i) ?: continue
                val name = part.optString("cl")
                val row = ContentValues().apply {
                    put("mid", mid)
                    put("ct", part.optString("ct"))
                    put("cl", name)
                    put("name", name)
                    put("cid", "<$name>")
                }
                val file = part.optString("file")
                if (file.isEmpty()) {
                    row.put("text", part.optString("text"))
                    row.put("chset", 106)
                }
                val saved = contentResolver.insert(Uri.parse("content://mms/$mid/part"), row) ?: error("no part")
                if (file.isNotEmpty()) {
                    contentResolver.openOutputStream(saved)?.use { out ->
                        File(inbox, file).inputStream().use { it.copyTo(out) }
                    } ?: error("no part file")
                }
            }
        } catch (_: Exception) {
            contentResolver.delete(uri, null, null)
            return false
        }
        return true
    }

    private fun label(pkg: String): String = try {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    } catch (_: PackageManager.NameNotFoundException) {
        pkg
    }

    private val BuildConfigVersion: String
        get() = packageManager.getPackageInfo(packageName, 0).versionName ?: ""

    private fun BufferedWriter.send(message: JSONObject) {
        write(message.toString())
        newLine()
        flush()
    }
}
