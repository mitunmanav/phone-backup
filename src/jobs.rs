use crate::*;

impl App {
    pub(crate) fn start(&mut self, ctx: &egui::Context) {
        let phones = self.phones.clone().unwrap_or_default();
        let (dest, pick) = (self.dest.clone(), self.pick.clone());
        self.begin(Job::Backup);
        self.result = None;
        let fail = |e| Msg::Done(Err(e));
        self.spawn_job(ctx, fail, move |report| {
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
                backup::mtp_backup(&dest, &pick, report).map(|s| vec![s])
            } else {
                // One phone failing (say, unplugged) must not skip the others.
                let (mut done, mut failed) = (Vec::new(), Vec::new());
                for (serial, model) in &adb {
                    match backup::adb_backup(serial, model, &dest, pick.clone(), report) {
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
            Msg::Done(result)
        });
    }

    /// Marks a job as running, before its worker starts.
    pub(crate) fn begin(&mut self, job: Job) {
        self.busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        backup::PAUSE.store(false, Ordering::Relaxed);
        self.paused_at = None;
        self.job = job;
        self.status = "Starting…".into();
        self.now.clear();
        self.fraction = None;
        self.tiles = None;
        // An old copy or delete result would otherwise read as this job's.
        self.tidy_msg = None;
    }

    /// Runs a job on a worker and keeps the computer awake until it ends. Its result goes to the
    /// window last, after the job is marked finished. If the job panics, `fail` turns the panic
    /// into that job's error result, so the window never stays stuck on "busy".
    fn spawn_job(
        &self,
        ctx: &egui::Context,
        fail: impl FnOnce(String) -> Msg + Send + 'static,
        work: impl FnOnce(&dyn Fn(Msg)) -> Msg + Send + 'static,
    ) {
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let done = caught(|| work(&report)).unwrap_or_else(fail);
            backup::keep_awake(false);
            // A stop or pause left on would cut short the size checks that run between jobs.
            backup::STOP.store(false, Ordering::Relaxed);
            backup::PAUSE.store(false, Ordering::Relaxed);
            busy.store(false, Ordering::Relaxed);
            report(done);
        });
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
        let fail = |e| Msg::Restored(Err(e));
        self.spawn_job(ctx, fail, move |report| {
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
                report,
            );
            Msg::Restored(result)
        });
    }

    /// Re-hashes a backup's files in a worker and compares them with its checksums.
    pub(crate) fn verify(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Verify);
        self.verified = None;
        let failed = folder.clone();
        let fail = |e| Msg::Verified(failed, Err(e));
        self.spawn_job(ctx, fail, move |report| {
            let result = backup::verify(&folder, report);
            Msg::Verified(folder, result)
        });
    }

    /// Exports the backup's contacts, calls, messages, calendar and photos to its exports folder,
    /// then opens that folder.
    pub(crate) fn export(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Export);
        self.export_msg = None;
        let out = folder.join(export::EXPORTS);
        let failed = out.clone();
        let fail = |e| Msg::Exported(failed, Err(e));
        self.spawn_job(ctx, fail, move |report| {
            let result = export::export(&folder, report);
            if result.is_ok() {
                let _ = Command::new("explorer").arg(&out).spawn();
            }
            Msg::Exported(out, result)
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
        let (dest, safety) = (self.dest.clone(), self.safety);
        match plan {
            Some(((_, folder, _, _, Some(ran)), false)) => {
                // Files chosen in the picker narrow what the check backed up. Apps are not in
                // the picker, so they stay.
                let ran = match self.only.as_ref().filter(|(f, ..)| *f == folder) {
                    Some((_, picked, _)) => {
                        let apps = folder.join("apps");
                        Arc::new(
                            ran.iter()
                                .filter(|f| picked.contains(*f) || f.starts_with(&apps))
                                .cloned()
                                .collect(),
                        )
                    }
                    None => ran,
                };
                self.checking = None;
                self.restore_report = Some(folder.join(backup::RESTORE_REPORT));
                let fail = |e| Msg::Restored(Err(e));
                self.spawn_job(ctx, fail, move |report| {
                    let r = backup::transfer_now(
                        (&to, &to_model),
                        &dest,
                        &folder,
                        &ran,
                        pick,
                        safety,
                        report,
                    );
                    Msg::Restored(r)
                });
            }
            _ => {
                // The folder is filled in when the backup of the first phone has made it.
                self.checking = Some((to.clone(), PathBuf::new(), pick.files, pick.apps, None));
                self.restore_report = None;
                let fail = |e| Msg::Transferred(None, Err(e));
                self.spawn_job(ctx, fail, move |report| {
                    let (folder, r) = backup::transfer_check(
                        (&from, &from_model),
                        (&to, &to_model),
                        &dest,
                        pick,
                        report,
                    );
                    Msg::Transferred(folder.map(|(f, ran)| (from, f, ran)), r)
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
        if job == Job::Delete {
            self.status = "Moving the backup to the Recycle Bin…".into();
        }
        let fail = |e| Msg::Tidied(Err(e));
        self.spawn_job(ctx, fail, move |report| {
            let r = match into {
                Some(into) => backup::merge_backups(&from, &into, report),
                None => backup::delete_backup(&from),
            };
            Msg::Tidied(r)
        });
    }
}

/// Runs `f`, turning a panic into a plain error line for the window.
pub(crate) fn caught<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|e| {
        let why = e
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        format!("Stopped because of an internal error ({why}). Please report it.")
    })
}

#[cfg(test)]
mod tests {
    use super::caught;

    #[test]
    fn caught_turns_a_panic_into_an_error() {
        assert_eq!(caught(|| 1), Ok(1));
        let e = caught(|| -> u8 { panic!("boom") }).unwrap_err();
        assert!(e.contains("boom") && e.contains("Please report it"), "{e}");
    }
}
