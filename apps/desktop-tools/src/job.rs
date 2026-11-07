//! Kill-on-close job object: if the host exits or crashes, Windows terminates
//! every plugin process assigned to it. Explicit restarts still use
//! `taskkill /T` so a single child tree can be replaced without the others.

use anyhow::{Context, Result};
use std::process::Child;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

pub struct Job(HANDLE);

impl Job {
    pub fn new() -> Result<Job> {
        let job = unsafe { CreateJobObjectW(None, None) }.context("CreateJobObjectW")?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if let Err(e) = ok {
            unsafe {
                let _ = CloseHandle(job);
            }
            return Err(e).context("SetInformationJobObject");
        }
        Ok(Job(job))
    }

    pub fn assign(&self, child: &Child) -> Result<()> {
        let process =
            unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id()) }
                .context("OpenProcess")?;
        let result = unsafe { AssignProcessToJobObject(self.0, process) };
        unsafe {
            let _ = CloseHandle(process);
        }
        result.context("AssignProcessToJobObject")
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
