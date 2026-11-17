//! A sample of a process that stopped answering.
//!
//! The monitor watches from outside, so it can look at a program that is not
//! answering: the thread table says how much time each thread used, and the
//! dump file holds the stacks, so the reader can see where the program was
//! when it stopped.
//!
//! Nothing here ends the process. The program is only read, so a process that
//! is stuck keeps its bytes and its threads exactly as they were.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// One thread of the sampled process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSample {
    /// The thread's own id.
    pub id: u32,
    /// How much time the thread has used, in the kernel and in user code.
    pub cpu: Duration,
}

/// What was seen of the frozen process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSample {
    /// The threads the process had.
    pub threads: Vec<ThreadSample>,
    /// The file the stacks were written to, when they could be written.
    pub dump: Option<PathBuf>,
}

impl ProcessSample {
    /// The thread table as it is written into a report.
    pub fn table(&self) -> Vec<String> {
        self.threads
            .iter()
            .map(|thread| {
                format!(
                    "thread {:>6} used {:.1}s of processor time",
                    thread.id,
                    thread.cpu.as_secs_f64()
                )
            })
            .collect()
    }
}

/// Take a sample of the process, writing the stacks to `dump` when a file is
/// named.
///
/// `None` when the process cannot be read at all: a program that has already
/// ended, or one the monitor may not look into.
#[cfg(target_os = "windows")]
pub fn sample(pid: u32, dump: Option<&Path>) -> Option<ProcessSample> {
    let threads = threads_of(pid);
    let dump_path = dump.and_then(|path| unsafe { write_dump(pid, path) });
    Some(ProcessSample {
        threads,
        dump: dump_path,
    })
}

/// The threads of the process, with the time each has used.
#[cfg(target_os = "windows")]
fn threads_of(pid: u32) -> Vec<ThreadSample> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };

    /// The hundred-nanosecond ticks a file time counts in.
    fn ticks(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
        ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
    }

    /// The time one thread has used.
    unsafe fn cpu_of(thread_id: u32) -> Option<Duration> {
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
        use windows_sys::Win32::System::Threading::{
            GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
        };
        let handle: HANDLE = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, thread_id);
        if handle.is_null() {
            return None;
        }
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let ok = GetThreadTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        let _ = CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let hundred_nanos = ticks(kernel) + ticks(user);
        Some(Duration::from_nanos(hundred_nanos * 100))
    }

    let mut found = Vec::new();
    unsafe {
        let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            cntUsage: 0,
            th32ThreadID: 0,
            th32OwnerProcessID: 0,
            tpBasePri: 0,
            tpDeltaPri: 0,
            dwFlags: 0,
        };
        let mut more = Thread32First(snapshot, &mut entry);
        while more != 0 {
            if entry.th32OwnerProcessID == pid {
                if let Some(cpu) = cpu_of(entry.th32ThreadID) {
                    found.push(ThreadSample {
                        id: entry.th32ThreadID,
                        cpu,
                    });
                }
            }
            more = Thread32Next(snapshot, &mut entry);
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

/// Write the stacks of the process to a file.
#[cfg(target_os = "windows")]
unsafe fn write_dump(pid: u32, path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        MiniDumpNormal, MiniDumpWithThreadInfo, MiniDumpWriteDump,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    /// The right to write the file the dump goes into.
    const GENERIC_WRITE: u32 = 0x4000_0000;
    /// Replace a file that is already there.
    const CREATE_ALWAYS: u32 = 2;

    let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
    if process.is_null() {
        return None;
    }
    let name: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let file: HANDLE = CreateFileW(
        name.as_ptr(),
        GENERIC_WRITE,
        FILE_SHARE_READ,
        std::ptr::null(),
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        std::ptr::null_mut(),
    );
    if file == INVALID_HANDLE_VALUE {
        let _ = CloseHandle(process);
        return None;
    }
    // The dump carries the threads and their stacks: what the program was
    // doing when it stopped answering.
    let written = MiniDumpWriteDump(
        process,
        pid,
        file,
        MiniDumpNormal | MiniDumpWithThreadInfo,
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
    );
    let _ = CloseHandle(file);
    let _ = CloseHandle(process);
    (written != 0).then(|| path.to_path_buf())
}

/// Take a sample of the process, writing the stacks to `dump` when a file is
/// named.
///
/// Without the platform's process API there is nothing to read, so no sample
/// is taken.
#[cfg(not(target_os = "windows"))]
pub fn sample(_pid: u32, _dump: Option<&Path>) -> Option<ProcessSample> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_process_can_be_sampled_without_ending_it() {
        let sample = sample(std::process::id(), None);
        assert!(sample.is_some(), "the running process can be read");
        let sample = sample.expect("a sample");
        assert!(
            !sample.threads.is_empty(),
            "a process has at least the thread that is running this test"
        );
    }

    #[test]
    fn the_table_reads_as_lines() {
        let sample = ProcessSample {
            threads: vec![ThreadSample {
                id: 4242,
                cpu: Duration::from_millis(1500),
            }],
            dump: None,
        };
        let table = sample.table();
        assert_eq!(table.len(), 1);
        assert!(table[0].contains("4242"), "{}", table[0]);
        assert!(table[0].contains("1.5"), "{}", table[0]);
    }

    #[test]
    fn a_process_that_is_not_there_has_no_sample() {
        // A pid that cannot be a running process.
        let sample = sample(0xFFFF_FFFE, None);
        assert!(sample.is_none() || sample.expect("a sample").threads.is_empty());
    }
}
