#!/bin/sh
# Puts awkward files, junk, a contact, a calendar event and a picture message on a test phone or emulator for the e2e tests:
#   adb shell < docs/e2e-seed.sh
#   adb connect 127.0.0.1:5555   (emulator only; tap Allow on its screen)
#   PB_E2E_SERIAL=emulator-5554 cargo test -- --include-ignored --test-threads=1
# Shared storage refuses : ? * < > | and \ in names, and ignores letter case.
B=/sdcard/E2E
rm -rf $B
mkdir -p $B/Camera $B/Docs "$B/sp ace dir" $B/.thumbnails $B/many
echo one > $B/Camera/IMG.jpg
echo two2 > $B/Camera/img.jpg
echo three > "$B/Camera/img (2).jpg"
dd if=/dev/urandom of=$B/Camera/big.bin bs=1048576 count=20 2>/dev/null
echo q > "$B/Docs/it's \$x.txt"
echo c2 > $B/Docs/a_b.txt
echo d > "$B/Docs/trailing dot."
echo e > $B/Docs/CON.txt
echo u > "$B/Docs/ünïcødé 日本.txt"
: > $B/Docs/empty.bin
echo lead > "$B/Docs/ leading space.txt"
echo dash > $B/Docs/-dash.txt
echo s > "$B/sp ace dir/x y.txt"
echo t > $B/.thumbnails/t.jpg
for i in $(seq 1 450); do echo $i > $B/many/f$i.txt; done
D=$B/deep
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do D="$D/folder_with_a_fairly_long_name_$i"; done
mkdir -p "$D" && echo deep > "$D/deepfile.txt"
content insert --uri content://com.android.contacts/raw_contacts --bind account_type:n: --bind account_name:n:
ID=$(content query --uri content://com.android.contacts/raw_contacts --projection _id | tail -1 | sed 's/.*_id=//')
content insert --uri content://com.android.contacts/data --bind raw_contact_id:i:$ID --bind mimetype:s:vnd.android.cursor.item/name --bind data1:s:'Ann, Lee; Jr'
content insert --uri content://com.android.contacts/data --bind raw_contact_id:i:$ID --bind mimetype:s:vnd.android.cursor.item/phone_v2 --bind data1:s:'+1 555 0100'
content insert --uri content://com.android.contacts/data --bind raw_contact_id:i:$ID --bind mimetype:s:vnd.android.cursor.item/email_v2 --bind data1:s:'ann@x.com'
# Junk the backup skips: a hidden cache, WhatsApp stickers and its copies of sent media.
mkdir -p $B/.cache "$B/WhatsApp/Media/WhatsApp Stickers" "$B/WhatsApp/Media/WhatsApp Images/Sent"
echo c > $B/.cache/c.bin
echo k > "$B/WhatsApp/Media/WhatsApp Stickers/s.webp"
echo w > "$B/WhatsApp/Media/WhatsApp Images/Sent/IMG-sent.jpg"
echo r > "$B/WhatsApp/Media/WhatsApp Images/IMG-received.jpg"
# One calendar event, in a local calendar made for it if the phone has none.
content delete --uri content://com.android.calendar/events --where "title='E2E dentist'"
CAL=$(content query --uri content://com.android.calendar/calendars --projection _id | grep _id= | head -1 | sed 's/.*_id=//')
if [ -z "$CAL" ]; then
  content insert --uri "content://com.android.calendar/calendars?caller_is_syncadapter=true&account_name=e2e&account_type=LOCAL" --bind account_name:s:e2e --bind account_type:s:LOCAL --bind name:s:E2E --bind calendar_displayName:s:E2E --bind calendar_access_level:i:700 --bind ownerAccount:s:e2e
  CAL=$(content query --uri content://com.android.calendar/calendars --projection _id | grep _id= | head -1 | sed 's/.*_id=//')
fi
content insert --uri content://com.android.calendar/events --bind calendar_id:i:$CAL --bind title:s:'E2E dentist' --bind dtstart:l:1767225600000 --bind dtend:l:1767229200000 --bind eventTimezone:s:UTC
# One picture message with text, a sender and a picture. Only the default SMS app may write
# messages, so the shell is allowed to first.
appops set com.android.shell WRITE_SMS allow
content delete --uri content://mms --where "sub='E2E'"
content insert --uri content://mms --bind msg_box:i:1 --bind date:l:1767225600 --bind sub:s:E2E --bind m_type:i:132
M=$(content query --uri content://mms --projection _id --where "sub='E2E'" | tail -1 | sed 's/.*_id=//')
content insert --uri content://mms/$M/part --bind ct:s:text/plain --bind text:s:'Hello from MMS'
content insert --uri content://mms/$M/part --bind ct:s:image/jpeg --bind cl:s:e2e.jpg
content insert --uri content://mms/$M/addr --bind address:s:+15550100 --bind type:i:137 --bind charset:i:106
P=$(content query --uri content://mms/part --projection _id --where "cl='e2e.jpg'" | tail -1 | sed 's/.*_id=//')
echo JPEGDATA | content write --uri content://mms/part/$P
