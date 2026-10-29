#!/bin/sh
# Puts awkward files and one contact on a test phone or emulator for the e2e tests:
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
