//! The state of one download: its segments, progress, control and metadata.

use super::*;
use super::resume::read_manifest;

impl Download {
    pub fn new(
        id: u64,
        url: String,
        dest: PathBuf,
        connections: u32,
        proxy: Option<&str>,
        referer: Option<&str>,
    ) -> Result<Arc<Download>, String> {
        Download::new_in(id, url, dest, None, connections, proxy, referer)
    }

    /// Create a download whose parts and manifest live in `unfinished`.
    ///
    /// The destination folder is created as well: the finished file is moved
    /// there, and a folder that does not exist yet would fail that move.
    pub fn new_in(
        id: u64,
        url: String,
        dest: PathBuf,
        unfinished: Option<&Path>,
        connections: u32,
        proxy: Option<&str>,
        referer: Option<&str>,
    ) -> Result<Arc<Download>, String> {
        Self::new_in_with_headers(id, url, dest, unfinished, connections, proxy, referer, &[])
    }

    /// Create a download that also carries the headers the page's browser
    /// sent. The caller has already scoped them to the manifest's origin.
    #[allow(clippy::too_many_arguments)]
    pub fn new_in_with_headers(
        id: u64,
        url: String,
        dest: PathBuf,
        unfinished: Option<&Path>,
        connections: u32,
        proxy: Option<&str>,
        referer: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<Arc<Download>, String> {
        let finish = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = unfinished
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| finish.clone());
        // The download owns its work folder only when it is not the folder
        // the finished file lands in.
        let staged = dir != finish;
        std::fs::create_dir_all(&finish)
            .map_err(|e| format!("cannot create {}: {e}", finish.display()))?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

        let stem = dest
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("download{id}"));
        let manifest = dir.join(format!("{stem}.idm.json"));

        let source = Source::parse_with_headers(&url, proxy, referer, headers)?;
        let referer = referer
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());

        let last_emit = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        Ok(Arc::new(Download {
            id,
            url,
            dest,
            dir,
            stem,
            manifest,
            staged,
            total: AtomicU64::new(0),
            max_connections: connections.max(1),
            segments: Mutex::new(Vec::new()),
            status: Mutex::new(Status::Queued),
            error: Mutex::new(None),
            control: Control::new(),
            next_segment_id: AtomicU32::new(0),
            source,
            referer,
            limiter: Arc::new(SpeedLimiter::unlimited()),
            resumable: Mutex::new(None),
            validator: Mutex::new(None),
            driver: AsyncMutex::new(()),
            head: Mutex::new(Vec::new()),
            degraded: AtomicBool::new(false),
            max_attempts: AtomicU32::new(MAX_ATTEMPTS),
            epoch: AtomicU64::new(0),
            cancelled_epoch: AtomicU64::new(0),
            stopped_epoch: AtomicU64::new(0),
            run_requests: AtomicU64::new(0),
            cancelled_request: AtomicU64::new(0),
            running: AtomicBool::new(false),
            discarded: AtomicBool::new(false),
            allow_replace: AtomicBool::new(false),
            category: Mutex::new(None),
            description: Mutex::new(None),
            last_emit: Mutex::new(last_emit),
            pace: Mutex::new(crate::pace::Watch::new()),
            learned_connections: Mutex::new(None),
            country: Mutex::new(None),
            rtt_ms: Mutex::new(None),
            redirected: Mutex::new(Vec::new()),
            last_status: AtomicU16::new(0),
        }))
    }

    /// Record the status the server last refused a request with.
    ///
    /// `None` forgets it: a run that reaches the server again starts from
    /// "nothing was refused" rather than from the answer of the last run.
    pub fn note_status(&self, status: Option<u16>) {
        self.last_status
            .store(status.unwrap_or(0), Ordering::SeqCst);
    }

    /// The status the server last refused a request with, if one did.
    ///
    /// The engine renders a refusal into a message for the row, and that
    /// message is all the media layer used to receive. A refusal that means
    /// "this address is no longer accepted" is a decision, not a sentence, so
    /// the code is kept beside the message.
    pub fn last_status(&self) -> Option<u16> {
        match self.last_status.load(Ordering::SeqCst) {
            0 => None,
            status => Some(status),
        }
    }

    pub fn info(&self) -> DownloadInfo {
        let total = self.total.load(Ordering::SeqCst);
        let (mut downloaded, connections, segments) = {
            let segments = self.segments.lock().expect("segments lock poisoned");
            (
                segments.iter().map(|s| s.downloaded).sum::<u64>(),
                segments.len() as u32,
                segments
                    .iter()
                    .map(|s| SegmentInfo {
                        id: s.id,
                        start: s.start,
                        size: if s.is_unknown() { None } else { Some(s.len()) },
                        downloaded: s.downloaded,
                        status: s.status,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let status = *self.status.lock().expect("status lock poisoned");
        let error = self.error.lock().expect("error lock poisoned").clone();
        let resumable = *self.resumable.lock().expect("resumable lock poisoned");

        // A finished file is finished. Its ranges were joined and its part
        // files and manifest removed, so the segment list of a download
        // restored from the index adds up to nothing; without this the row
        // would show the full size and 0% of it.
        if status == Status::Completed {
            downloaded = downloaded.max(total);
        }

        DownloadInfo {
            id: self.id,
            url: self.url.clone(),
            dest: self.dest.to_string_lossy().to_string(),
            total,
            downloaded,
            connections,
            status,
            error,
            referer: self.referer.clone(),
            segments,
            resumable,
            speed_limit: self.limiter.rate(),
            category: self
                .category
                .lock()
                .expect("category lock poisoned")
                .clone(),
            description: self
                .description
                .lock()
                .expect("description lock poisoned")
                .clone(),
            learned_connections: *self
                .learned_connections
                .lock()
                .expect("learned connections lock poisoned"),
            country: self.country.lock().expect("country lock poisoned").clone(),
            rtt_ms: *self.rtt_ms.lock().expect("rtt lock poisoned"),
        }
    }

    /// The bytes that have arrived, without building the whole row.
    ///
    /// The tray tooltip is written on every progress report, and it shows
    /// only a number. Building the row of every download for it would copy
    /// every range of every download several times a second, and the copy
    /// takes the same lock the engine takes when it records a byte.
    pub fn downloaded(&self) -> u64 {
        self.segments
            .lock()
            .expect("segments lock poisoned")
            .iter()
            .map(|segment| segment.downloaded)
            .sum()
    }

    /// Record the category and the note the user gave the download.
    ///
    /// Both come from a text field, so they are trimmed and cut to the length
    /// the index will hold; an empty value is no value.
    pub fn set_meta(&self, category: Option<String>, description: Option<String>) {
        self.set_category(category);
        self.set_description(description);
    }

    /// Record the category alone, leaving the note as it is.
    ///
    /// Moving a download between categories is metadata: the destination and
    /// everything on disk stay untouched.
    pub fn set_category(&self, category: Option<String>) {
        *self.category.lock().expect("category lock poisoned") = clean_meta(category, 40);
    }

    /// Record the note alone, leaving the category as it is.
    pub fn set_description(&self, description: Option<String>) {
        *self.description.lock().expect("description lock poisoned") = clean_meta(description, 500);
    }

    /// The bandwidth cap of this download, in bytes per second. Zero means no
    /// cap.
    pub fn speed_limit(&self) -> u64 {
        self.limiter.rate()
    }

    /// Record whether the file can be resumed, once the probe has answered.
    pub fn set_resumable(&self, resumable: bool) {
        *self.resumable.lock().expect("resumable lock poisoned") = Some(resumable);
    }

    /// Record the validator the probe was answered with.
    pub(super) fn set_validator(&self, validator: Option<String>) {
        *self.validator.lock().expect("validator lock poisoned") = validator;
    }

    /// The validator the probe was answered with, when there is one.
    pub(super) fn validator(&self) -> Option<String> {
        self.validator
            .lock()
            .expect("validator lock poisoned")
            .clone()
    }

    /// Allow the destination file to be replaced when the transfer finishes.
    ///
    /// This is set only from an explicit Replace; every other start leaves it
    /// off, and `assemble` refuses to overwrite a file that appeared while the
    /// download was running.
    pub fn set_allow_replace(&self, allow: bool) {
        self.allow_replace.store(allow, Ordering::SeqCst);
    }

    /// Forget every byte received so far and remove the resume state.
    ///
    /// Used by Retry when the server cannot resume: the next run plans fresh
    /// ranges instead of continuing from parts that cannot be continued.
    pub fn discard_progress(&self) -> Result<(), String> {
        if self.is_running() {
            return Err("the download is still running".to_string());
        }
        remove_orphan_parts(&self.dir, &self.stem, &[]);
        let _ = std::fs::remove_file(join_temp_path(&self.dir, &self.stem));
        let _ = std::fs::remove_file(manifest_temp_path(&self.manifest));
        let _ = std::fs::remove_file(&self.manifest);
        self.segments
            .lock()
            .expect("segments lock poisoned")
            .clear();
        self.total.store(0, Ordering::SeqCst);
        self.next_segment_id.store(0, Ordering::SeqCst);
        self.head.lock().expect("head lock poisoned").clear();
        self.degraded.store(false, Ordering::SeqCst);
        *self.resumable.lock().expect("resumable lock poisoned") = None;
        self.set_validator(None);
        Ok(())
    }

    /// Remove the parts and the manifest of a download that is leaving the list.
    ///
    /// A row that no window can show has nothing left to resume, so its bytes
    /// go with it. A task that is still winding down writes the manifest back
    /// at its end; the flag makes it clean up there instead. A staged
    /// download also loses the folder it owned, when nothing is left in it.
    pub fn forget_progress(&self) {
        self.discarded.store(true, Ordering::SeqCst);
        remove_orphan_parts(&self.dir, &self.stem, &[]);
        let _ = std::fs::remove_file(join_temp_path(&self.dir, &self.stem));
        let _ = std::fs::remove_file(manifest_temp_path(&self.manifest));
        let _ = std::fs::remove_file(&self.manifest);
        self.prune_staged_dir();
    }

    /// Remove this download's work folder, when it owns one and it is empty.
    ///
    /// The folder is the application's only when it is not the destination's;
    /// `remove_dir` refuses a folder that still holds bytes or files, so a
    /// task winding down late keeps its own folder until it is done.
    pub(super) fn prune_staged_dir(&self) {
        if self.staged {
            let _ = std::fs::remove_dir(&self.dir);
        }
    }

    /// The epoch of the newest task of this download.
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// The state the engine reports for this download.
    pub fn status(&self) -> Status {
        *self.status.lock().expect("status lock poisoned")
    }

    pub fn set_status(&self, status: Status) {
        *self.status.lock().expect("status lock poisoned") = status;
    }

    /// Forget the message of the last failure, before a new attempt starts.
    pub fn clear_error(&self) {
        *self.error.lock().expect("error lock poisoned") = None;
    }

    /// Cap the bandwidth of this download. Zero means no cap.
    pub fn set_speed_limit(&self, bytes_per_second: u64) {
        self.limiter.set_rate(bytes_per_second);
    }

    /// Hold every connection of this download back.
    ///
    /// The connections are marked here as well as told to wait, so the table
    /// of the progress window says `Paused` at once instead of waiting for
    /// every stream to notice.
    pub fn pause(&self) {
        self.control.pause();
        self.mark_segments(SegmentStatus::Paused, true);
    }

    /// Start a new task for this download and return its epoch.
    ///
    /// A task that is already running is left alone: its epoch is older than
    /// the new one, so a cancel still reaches it.
    pub fn next_epoch(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Ask for a new run of this download.
    ///
    /// The request is claimed here rather than inside the task, because a
    /// cancel can arrive while the task is still waiting for the previous run
    /// to let go of the driver. A request that a cancel has covered must not
    /// start afterwards.
    pub fn request_run(&self) -> u64 {
        self.run_requests.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether a run request was stopped or replaced before it started.
    pub fn is_superseded(&self, request: u64) -> bool {
        request <= self.cancelled_request.load(Ordering::SeqCst)
            || request < self.run_requests.load(Ordering::SeqCst)
    }

    /// Stop the task that runs this download.
    pub fn cancel(&self) {
        let current = self.epoch.load(Ordering::SeqCst);
        self.cancelled_epoch.fetch_max(current, Ordering::SeqCst);
        // Every run request made so far is covered by this cancel, including
        // one whose task has not reached the driver yet.
        let request = self.run_requests.load(Ordering::SeqCst);
        self.cancelled_request.fetch_max(request, Ordering::SeqCst);
        // A paused range waits on the pause channel, not on the network, so a
        // cancel has to release it or the task would wait forever. The recheck
        // of the epoch in every loop is what stops it once it wakes.
        self.control.resume();
    }

    /// Stop the tasks of this download without calling it cancelled.
    ///
    /// This is what the engine does when one range fails: the other ranges
    /// stop, and the download is reported as failed, not as cancelled.
    pub fn stop(&self) {
        let current = self.epoch.load(Ordering::SeqCst);
        self.stopped_epoch.fetch_max(current, Ordering::SeqCst);
        self.control.resume();
    }

    /// Stop this download and say why, in one sentence.
    ///
    /// Used when the network the reader asked for is gone: the bytes stop,
    /// and the row says what happened instead of a connection error nobody
    /// can make sense of.
    pub fn fail_with(&self, reason: impl Into<String>) {
        self.stop();
        *self.error.lock().expect("error lock poisoned") = Some(reason.into());
        self.set_status(Status::Failed);
    }

    /// Whether the task with this epoch was stopped.
    pub fn is_cancelled(&self, epoch: u64) -> bool {
        epoch <= self.cancelled_epoch.load(Ordering::SeqCst)
            || epoch <= self.stopped_epoch.load(Ordering::SeqCst)
    }

    /// Whether the task with this epoch was cancelled by the user.
    pub fn is_cancelled_by_user(&self, epoch: u64) -> bool {
        epoch <= self.cancelled_epoch.load(Ordering::SeqCst)
    }

    /// Whether a task is running this download right now.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Rebuild a download the app remembers from an earlier run.
    ///
    /// The parts and the manifest are still on disk, so the download can be
    /// resumed. It comes back paused: nothing starts on its own.
    pub fn from_known(known: &crate::session::KnownDownload) -> Option<Arc<Download>> {
        let download = Download::new_in(
            known.id,
            known.url.clone(),
            PathBuf::from(&known.dest),
            known.unfinished_dir.as_deref().map(Path::new),
            known.connections,
            None,
            known.referer.as_deref(),
        )
        .ok()?;

        if let Some(manifest) = read_manifest(&download) {
            let validator = manifest.validator.clone();
            download.total.store(manifest.total, Ordering::SeqCst);
            let next_id = manifest.segments.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            download.next_segment_id.store(next_id, Ordering::SeqCst);
            *download.segments.lock().expect("segments lock poisoned") = manifest.segments;
            // The validator outlives the process with the plan it belongs to.
            download.set_validator(validator);
        } else {
            download.total.store(known.total, Ordering::SeqCst);
        }

        // The manifest on disk is the proof that the file can be resumed.
        if download.manifest.exists() {
            download.set_resumable(true);
        }
        download.set_speed_limit(known.speed_limit);
        download.set_meta(known.category.clone(), known.description.clone());
        // The Replace answer outlives the process: the file it replaces is
        // still there, and the finished download has to be allowed to put
        // itself over it.
        download.set_allow_replace(known.replace);
        download.reset_segment_statuses();

        download.set_status(match known.status {
            Status::Completed => Status::Completed,
            Status::Failed => Status::Failed,
            _ => Status::Paused,
        });
        *download.error.lock().expect("error lock poisoned") = known.error.clone();

        Some(download)
    }

    /// Write the resume state of this download to disk.
    pub fn save_manifest(&self) {
        save_manifest(self);
    }

    /// The index entry for this download.
    pub fn known(&self) -> crate::session::KnownDownload {
        let info = self.info();
        crate::session::KnownDownload {
            id: info.id,
            url: info.url,
            dest: info.dest,
            connections: self.max_connections,
            total: info.total,
            downloaded: info.downloaded,
            status: info.status,
            error: info.error,
            referer: self.referer.clone(),
            speed_limit: self.speed_limit(),
            category: info.category,
            description: info.description,
            replace: self.allow_replace.load(Ordering::SeqCst),
            unfinished_dir: Some(self.dir.to_string_lossy().to_string()),
        }
    }

    pub(super) fn seg_snapshot(&self, id: u32) -> Option<(u64, u64, u64)> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments
            .iter()
            .find(|s| s.id == id)
            .map(|s| (s.start, s.end, s.downloaded))
    }

    pub(super) fn seg_part(&self, id: u32) -> Option<PathBuf> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments.iter().find(|s| s.id == id).map(|s| s.part.clone())
    }

    pub(super) fn set_seg_downloaded(&self, id: u32, value: u64) {
        self.record_progress(id, value, SegmentStatus::Receiving);
    }

    /// Record how far a connection has got and what it is doing, in one lock.
    pub(super) fn record_progress(&self, id: u32, downloaded: u64, status: SegmentStatus) {
        let recorded = {
            let mut segments = self.segments.lock().expect("segments lock poisoned");
            match segments.iter_mut().find(|s| s.id == id) {
                Some(segment) => {
                    segment.downloaded = downloaded.min(segment.len());
                    segment.status = status;
                    segment.downloaded
                }
                None => return,
            }
        };
        // The pacing rules read what each range has taken. The segments lock
        // is released first: the rules take their own snapshot.
        self.pace
            .lock()
            .expect("pace lock poisoned")
            .record(id, recorded);
    }

    /// Keep the cap this host allowed the last time, for the row to show.
    pub(super) fn set_learned_connections(&self, connections: Option<u32>) {
        *self
            .learned_connections
            .lock()
            .expect("learned connections lock poisoned") = connections;
    }

    /// Record where the server that answers is.
    pub(super) fn set_location(&self, country: Option<String>, rtt_ms: Option<u64>) {
        *self.country.lock().expect("country lock poisoned") = country;
        *self.rtt_ms.lock().expect("rtt lock poisoned") = rtt_ms;
    }

    /// Set the state of one connection.
    pub(super) fn set_seg_status(&self, id: u32, status: SegmentStatus) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            segment.status = status;
        }
    }

    /// Mark every connection of the plan. Ranges that are already complete
    /// keep their state.
    pub(super) fn mark_segments(&self, status: SegmentStatus, only_unfinished: bool) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        for segment in segments.iter_mut() {
            if only_unfinished && segment.done() {
                segment.status = SegmentStatus::Done;
            } else {
                segment.status = status;
            }
        }
    }

    /// A download that is read back from disk has no connections open, so its
    /// ranges are waiting. The bytes they hold are still there.
    fn reset_segment_statuses(&self) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        for segment in segments.iter_mut() {
            segment.status = if segment.done() {
                SegmentStatus::Done
            } else {
                SegmentStatus::Pending
            };
        }
    }

    /// Close a segment whose length the server never told us: the bytes that
    /// arrived are the whole file.
    pub(super) fn finish_unknown_segment(&self, id: u32) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            if segment.end == UNKNOWN_END {
                segment.end = segment.start + segment.downloaded;
                segment.status = SegmentStatus::Done;
            }
        }
    }
}
