use crate::*;

/// Test data for checking the layout against realistic worst cases. Never shown in release builds.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Fixture {
    Real,
    Worst,
    Empty,
    One,
    Many,
}

impl App {
    pub(crate) fn load_fixture(&mut self, f: Fixture) {
        self.fixture = f;
        let saved = |name: &str, files: bool, apps: bool, age: Option<u64>| backup::Saved {
            path: PathBuf::from(format!(
                r"C:\fixture\{name}_R5CT{:04X}",
                age.unwrap_or(7) * 31
            )),
            name: name.into(),
            files,
            apps,
            age_days: age,
            // A size not added up yet, on the backup of unknown date.
            bytes: age.map(|a| 2_147_483_648 + a * 937_000_123),
        };
        let adb = |model: &str, wifi: bool, battery: Option<u8>| Phone::Adb {
            serial: format!("{model}-serial"),
            model: model.into(),
            wifi,
            battery,
        };
        self.restore_msg = None;
        self.export_msg = None;
        self.result = None;
        self.wifi_msg = None;
        self.verified = None;
        match f {
            Fixture::Real => {
                self.phones = None;
                self.backups.clear();
                let dest = self.dest.clone();
                self.scan(&dest);
            }
            Fixture::Worst => {
                self.phones = Some(vec![adb("Redmi Note 13 Pro+ 5G", true, Some(9))]);
                self.dest = PathBuf::from(
                    r"C:\Users\Aleksandra Wiśniewska-Kowalczyk\OneDrive - Northwind Industries Holdings\Documents\Phone backups",
                );
                self.backups = vec![
                    saved("Redmi Note 13 Pro+ 5G", true, true, Some(0)),
                    // A second phone of the same model: same name, different ID.
                    saved("Redmi Note 13 Pro+ 5G", true, false, Some(1284)),
                    // A folder the user picked by hand, not named "<model>_<id>".
                    saved(
                        "Backup from old phone before factory reset (Aleksandra)",
                        true,
                        true,
                        None,
                    ),
                    saved("Jo", false, true, Some(1)),
                ];
                self.verified = Some((
                    self.backups[0].path.clone(),
                    Err("1,284 files are changed, for example files/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4, files/DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg, files/Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf.\n3 files are missing, for example data/sms.txt, apps/com.whatsapp/base.apk, files/ünïcødé 日本.txt.\nBack up again to copy them from the phone, if it still has them.".into()),
                ));
                let exports = self
                    .dest
                    .join("Redmi Note 13 Pro+ 5G_R5CT0000")
                    .join("exports");
                self.export_msg = Some((
                    exports.clone(),
                    Ok(format!(
                        "Exported contacts, call history, messages, calendar, photos and videos to {}.",
                        exports.display()
                    )),
                ));
                self.restore_msg = Some(Err("Copied 1 file back to the phone. 1,284 files were already there. 3 files could not be copied, for example /sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4, /sdcard/DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg, /sdcard/Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf. Restore again to retry.\nThe phone was disconnected. Files copied so far stay on the phone; plug it back in and restore again to continue.".into()));
                self.result = Some(Ok(vec![Summary {
                    name: "Redmi Note 13 Pro+ 5G".into(),
                    root: self.dest.clone(),
                    files: 128_456,
                    copied: 1,
                    errors: vec!["x".into()],
                    data: vec![
                        ("Contacts", true),
                        ("Call history", false),
                        ("Messages", false),
                    ],
                    full: true,
                    pick: Pick::default(),
                }]));
                self.wifi_msg = Some(backup::wifi_result(
                    "connect",
                    "failed to connect to 'aleksandras-redmi-note-13-pro-plus.northwind.local:41235': No connection could be made because the target machine actively refused it. (10061)",
                ));
                self.connect_addr =
                    "aleksandras-redmi-note-13-pro-plus.northwind.local:41235".into();
                let long = "Aleksandra's scanned university documents and receipts 2019-2024";
                let list = [
                    ("DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg", 48_213_000_000),
                    ("DCIM/Camera/VID_20240817.mp4", 96_100_000_000),
                    ("Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf", 1_200_000),
                    (&format!("{long}/scan.pdf"), 3_400_000_000),
                    ("Music/a.flac", 12_000_000_000),
                    ("Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-1.mp4", 7_000_000_000),
                    ("日本語のフォルダ/メモ.txt", 4_000),
                    ("notes.txt", 0),
                ]
                .iter()
                .map(|(p, s)| (p.to_string(), *s))
                .collect();
                self.sizes = Some((
                    "Redmi Note 13 Pro+ 5G-serial".into(),
                    Some((list, 21_400_000_000)),
                ));
                self.pick.folders = ["DCIM".to_string(), long.to_string(), String::new()].into();
                self.pick.kinds = [true, true, false, true, false];
                self.pick.max_size = Some(4_000_000_000);
                let root = self.backups[0].path.join("files");
                let at = |p: &str, n: u64| (root.join(p), n);
                self.show_dupes(vec![
                    vec![
                        at(r"DCIM\Camera\VID_20240817_183455123_HDR_ultra_long_name.mp4", 1_284_000_000),
                        at(r"Android\media\com.whatsapp\WhatsApp\Media\WhatsApp Video\Sent\VID-20240817-WA0007.mp4", 1_284_000_000),
                    ],
                    vec![
                        at(r"DCIM\Camera\IMG_1.jpg", 3_400_000),
                        at(r"Pictures\Telegram\IMG_1.jpg", 3_400_000),
                        at(r"Download\ünïcødé 日本 IMG_1.jpg", 3_400_000),
                    ],
                ]);
                self.tool_msg = Some(("contacts", Ok("Merged 1,284 contacts into 1,190, saved as contacts-merged.vcf next to contacts.vcf. The original is unchanged.".into())));
                self.free_pick = Some((self.backups[0].path.clone(), [root.join("x")].into()));
            }
            Fixture::Empty => {
                self.phones = Some(Vec::new());
                self.backups.clear();
            }
            Fixture::One => {
                self.phones = Some(vec![Phone::Mtp { name: "Jo".into() }]);
                self.backups = vec![saved("Jo", true, false, Some(1))];
            }
            Fixture::Many => {
                self.phones = Some(vec![
                    adb("Pixel 8", false, None),
                    adb("Galaxy S23 Ultra", false, Some(14)),
                    adb("Redmi Note 13 Pro+ 5G", true, None),
                ]);
                self.backups = (0..40)
                    .map(|i| saved(&format!("Pixel {}", i % 9), true, i % 3 == 0, Some(i * 17)))
                    .collect();
            }
        }
        self.chosen = self.backups.first().map(|b| b.path.clone());
        let first_adb = self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, .. } => Some(serial.clone()),
            _ => None,
        });
        self.preview = first_adb.zip(self.chosen.clone()).map(|(serial, path)| {
            let plan = (serial, path, true, true, None);
            (plan, Ok("Files: will copy 128,456 files (214.7 GB) to the phone. 1,284 files already on the phone will be skipped; nothing is replaced. This phone has no SD card named 3A1F-0C2B or 9C33-6BD5, so those files cannot be restored.\nApps: will install 212 of 214, for example com.northwindindustries.holdings.expensereports.enterprise, com.whatsapp, org.mozilla.firefox. The rest are already on the phone.".into()))
        });
        if f != Fixture::Real {
            self.scanned = self.dest.clone();
        }
    }
}
