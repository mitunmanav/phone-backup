use crate::*;

impl App {
    pub(crate) fn start(&mut self, ctx: &egui::Context) {
        let phones = self.phones.clone().unwrap_or_default();
        let (dest, pick, tx, ctx, busy) = (
            self.dest.clone(),
            self.pick.clone(),
            self.tx.clone(),
            ctx.clone(),
            self.busy.clone(),
        );
        self.begin(Job::Backup);
        self.result = None;
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let adb: Vec<_> = phones
                .iter()
                .filter_map(|p| match p {
                    Phone::Adb { serial, model, .. } => Some((serial, model)),
                    _ => None,
                })
                .collect();
            let result = if let Err(e) = fs::create_dir_all(&dest) {
                Err(format!(
                    "Cannot use the folder {}: {e}. Choose another folder.",
                    dest.display()
                ))
            } else if adb.is_empty() {
                backup::mtp_backup(&dest, &pick, &report).map(|s| vec![s])
            } else {
                // One phone failing (say, unplugged) must not skip the others.
                let (mut done, mut failed) = (Vec::new(), Vec::new());
                for (serial, model) in &adb {
                    match backup::adb_backup(serial, model, &dest, pick.clone(), &report) {
                        Ok(s) => done.push(s),
                        Err(e) if adb.len() > 1 => failed.push(format!("{model}: {e}")),
                        Err(e) => failed.push(e),
                    }
                }
                if failed.is_empty() {
                    Ok(done)
                } else {
                    let names: Vec<_> = done.iter().map(|s| s.name.clone()).collect();
                    let ok = if names.is_empty() {
                        String::new()
                    } else {
                        format!("\n{} backed up fine.", names.join(", "))
                    };
                    Err(failed.join("\n") + &ok)
                }
            };
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Done(result));
        });
    }

    /// Marks a job as running, before its worker starts.
    pub(crate) fn begin(&mut self, job: Job) {
        self.busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.job = job;
        self.status = "Starting…".into();
        self.now.clear();
        self.fraction = None;
    }

    /// The model of the ADB phone with this serial, or the serial when it is gone.
    pub(crate) fn model_of(&self, serial: &str) -> String {
        self.phones
            .iter()
            .flatten()
            .find_map(|p| match p {
                Phone::Adb {
                    serial: s, model, ..
                } if s == serial => Some(model.clone()),
                _ => None,
            })
            .unwrap_or_else(|| serial.to_string())
    }

    /// Restores photos and files first, then apps, from one backup folder.
    /// With `check`, changes nothing on the phone and reports what a restore would do.
    pub(crate) fn restore(&mut self, ctx: &egui::Context, plan: Plan, check: bool) {
        self.begin(Job::Restore);
        self.restore_msg = None;
        self.preview = None;
        self.checking = check.then(|| plan.clone());
        let (serial, folder, files, apps, only) = plan;
        let phone = self.model_of(&serial);
        self.restore_report = (!check).then(|| folder.join(backup::RESTORE_REPORT));
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let pick = Pick {
                files,
                apps,
                data: false,
                ..Pick::default()
            };
            let result = backup::restore(
                &serial,
                &phone,
                &folder,
                pick,
                only.as_deref(),
                check,
                &report,
            );
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Restored(result));
        });
    }

    /// Re-hashes a backup's files in a worker and compares them with its checksums.
    pub(crate) fn verify(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Verify);
        self.verified = None;
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let result = backup::verify(&folder, &report);
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Verified(folder, result));
        });
    }

    /// Exports the backup's contacts, calls, messages, calendar and photos to its exports folder,
    /// then opens that folder.
    pub(crate) fn export(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Export);
        self.export_msg = None;
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let result = export::export(&folder, &report);
            let out = folder.join(export::EXPORTS);
            if result.is_ok() {
                let _ = Command::new("explorer").arg(&out).spawn();
            }
            busy.store(false, Ordering::Relaxed);
            report(Msg::Exported(out, result));
        });
    }

    /// Phone to phone. Without a checked plan: backs up the first phone and checks what
    /// restoring it to the second would do. With one: the safety backup, then the restore.
    pub(crate) fn transfer(&mut self, ctx: &egui::Context, plan: Option<(Plan, bool)>) {
        let (Some(from), Some(to)) = (self.from.clone(), self.to.clone()) else {
            return;
        };
        let (from_model, to_model) = (self.model_of(&from), self.model_of(&to));
        let pick = Pick {
            files: self.restore_files,
            apps: self.restore_apps,
            data: false,
            ..Pick::default()
        };
        self.begin(Job::Restore);
        self.restore_msg = None;
        self.preview = None;
        let (tx, ctx, busy, dest, safety) = (
            self.tx.clone(),
            ctx.clone(),
            self.busy.clone(),
            self.dest.clone(),
            self.safety,
        );
        let report = move |m: Msg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        match plan {
            Some(((_, folder, _, _, Some(ran)), false)) => {
                self.checking = None;
                self.restore_report = Some(folder.join(backup::RESTORE_REPORT));
                thread::spawn(move || {
                    let r = backup::transfer_now(
                        (&to, &to_model),
                        &dest,
                        &folder,
                        &ran,
                        pick,
                        safety,
                        &report,
                    );
                    busy.store(false, Ordering::Relaxed);
                    report(Msg::Restored(r));
                });
            }
            _ => {
                // The folder is filled in when the backup of the first phone has made it.
                self.checking = Some((to.clone(), PathBuf::new(), pick.files, pick.apps, None));
                self.restore_report = None;
                thread::spawn(move || {
                    let (folder, r) = backup::transfer_check(
                        (&from, &from_model),
                        (&to, &to_model),
                        &dest,
                        pick,
                        &report,
                    );
                    busy.store(false, Ordering::Relaxed);
                    report(Msg::Transferred(folder.map(|(f, ran)| (from, f, ran)), r));
                });
            }
        }
    }

    /// Copies one backup into another, or deletes one, on a worker.
    pub(crate) fn tidy(
        &mut self,
        ctx: &egui::Context,
        job: Job,
        from: PathBuf,
        into: Option<PathBuf>,
    ) {
        self.begin(job);
        self.tidy_msg = None;
        if job == Job::Delete {
            self.status = "Moving the backup to the Recycle Bin…".into();
        }
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let r = match into {
                Some(into) => backup::merge_backups(&from, &into, &report),
                None => backup::delete_backup(&from),
            };
            busy.store(false, Ordering::Relaxed);
            report(Msg::Tidied(r));
        });
    }
}
