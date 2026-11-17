use std::io;
use std::process::ExitStatus;
use tokio::process::{Child, Command};

pub(super) struct ShellProcess {
    pub(super) child: Child,
    tree: platform::ProcessTree,
    armed: bool,
}

impl ShellProcess {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let tree = platform::ProcessTree::new()?;
        tree.configure(command);
        command.kill_on_drop(true);
        let mut process = Self {
            child: command.spawn()?,
            tree,
            armed: true,
        };
        process.tree.attach(&process.child)?;
        Ok(process)
    }

    pub(super) async fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait().await?;
        // Successful shell exit may leave deliberately backgrounded work alive.
        // On Unix, also stop retaining a PGID as soon as its leader is reaped.
        self.armed = false;
        self.tree.disarm()?;
        Ok(status)
    }

    pub(super) async fn kill(&mut self) -> io::Result<()> {
        self.tree.terminate()?;
        self.armed = false;
        self.tree.disarm()?;
        self.child.wait().await?;
        Ok(())
    }
}

impl Drop for ShellProcess {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.tree.terminate();
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;

    pub(super) struct ProcessTree {
        pgid: Option<libc::pid_t>,
    }

    impl ProcessTree {
        pub(super) fn new() -> io::Result<Self> {
            Ok(Self { pgid: None })
        }

        pub(super) fn configure(&self, command: &mut Command) {
            command.process_group(0);
        }

        pub(super) fn attach(&mut self, child: &Child) -> io::Result<()> {
            self.pgid = Some(child.id().expect("freshly spawned shell has a PID") as libc::pid_t);
            Ok(())
        }

        pub(super) fn terminate(&self) -> io::Result<()> {
            if let Some(pgid) = self.pgid {
                // SAFETY: this positive PGID belongs to the fresh process group
                // created for this invocation, never Goose's shared group.
                if unsafe { libc::kill(-pgid, libc::SIGKILL) } != 0 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(libc::ESRCH) {
                        return Err(error);
                    }
                }
            }
            Ok(())
        }

        pub(super) fn disarm(&mut self) -> io::Result<()> {
            self.pgid = None;
            Ok(())
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use ntapi::ntpsapi::NtResumeProcess;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use winapi::um::jobapi2::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, TerminateJobObject,
    };
    use winapi::um::winbase::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
    use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub(super) struct ProcessTree {
        job: OwnedHandle,
    }

    impl ProcessTree {
        pub(super) fn new() -> io::Result<Self> {
            // SAFETY: null attributes/name create an unnamed, noninheritable job.
            let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let tree = Self {
                // SAFETY: CreateJobObjectW returned a new handle owned here.
                job: unsafe { OwnedHandle::from_raw_handle(handle) },
            };
            tree.set_kill_on_close(true)?;
            Ok(tree)
        }

        pub(super) fn configure(&self, command: &mut Command) {
            // Assign before executing user code, so even immediately spawned
            // grandchildren inherit the job rather than escaping assignment.
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }

        pub(super) fn attach(&mut self, child: &Child) -> io::Result<()> {
            let process = child
                .raw_handle()
                .expect("freshly spawned shell has a handle");
            // SAFETY: both handles are live; the child is still suspended.
            if unsafe { AssignProcessToJobObject(self.job.as_raw_handle(), process) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // Tokio exposes the process handle, not the primary thread handle.
            // Resume the process only after it belongs to this invocation's job.
            // SAFETY: the suspended child process handle remains live for the call.
            let status = unsafe { NtResumeProcess(process) };
            if status < 0 {
                return Err(io::Error::other(format!(
                    "Failed to resume shell process: NTSTATUS {status:#x}"
                )));
            }
            Ok(())
        }

        pub(super) fn terminate(&self) -> io::Result<()> {
            // SAFETY: the handle identifies only this shell invocation's job.
            if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        fn set_kill_on_close(&self, enabled: bool) -> io::Result<()> {
            // SAFETY: zeroed is a valid initial limit structure, and its address
            // and exact size are provided only for the duration of this call.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            if enabled {
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            }
            if unsafe {
                SetInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &mut limits as *mut _ as *mut _,
                    std::mem::size_of_val(&limits) as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) fn disarm(&mut self) -> io::Result<()> {
            self.set_kill_on_close(false)
        }
    }
}
