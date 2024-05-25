use std::{ffi::c_void, ptr::null_mut};

use minhook_raw::sys::{MH_CreateHook, MH_QueueDisableHook, MH_QueueEnableHook, MH_STATUS};

pub struct MhHook {
    addr: *mut c_void,
    trampoline: *mut c_void,
}

impl MhHook {
    pub unsafe fn new(addr: *mut c_void, hook_impl: *mut c_void) -> Result<Self, MH_STATUS> {
        let mut trampoline = null_mut();

        let status = MH_CreateHook(addr, hook_impl, &mut trampoline);

        if status != MH_STATUS::MH_OK {
            return Err(status);
        }

        Ok(Self {
            addr,
            trampoline,
        })
    }

    pub fn trampoline(&self) -> *mut c_void {
        self.trampoline
    }

    pub unsafe fn queue_enable(&self) -> Result<(), MH_STATUS> {
        let status = MH_QueueEnableHook(self.addr);

        if status == MH_STATUS::MH_OK {
            Ok(())
        } else {
            Err(status)
        }
    }

    pub unsafe fn queue_disable(&self) -> Result<(), MH_STATUS> {
        let status = MH_QueueDisableHook(self.addr);

        if status == MH_STATUS::MH_OK {
            Ok(())
        } else {
            Err(status)
        }
    }
}
