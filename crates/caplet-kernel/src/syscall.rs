//! Syscall definitions

/// Syscall numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Syscall {
    /// Invalid syscall
    None = 0,
    /// Spawn a new task
    TaskSpawn = 1,
    /// Exit the current task
    TaskExit = 2,
    /// Yield to scheduler
    TaskYield = 3,
    /// Get current task ID
    TaskGetId = 4,
    /// Sleep for specified ticks
    TaskSleep = 5,
    /// Derive a new capability
    CapDerive = 10,
    /// Duplicate a capability
    CapDuplicate = 11,
    /// Revoke a capability
    CapRevoke = 12,
    /// Find a capability by name
    CapFind = 13,
    /// Grant capability to another task
    CapGrant = 14,
    /// Send IPC message
    IpcSend = 20,
    /// Receive IPC message
    IpcRecv = 21,
    /// Reply to IPC message
    IpcReply = 22,
    /// Create a channel
    ChannelCreate = 23,
    /// Send on channel
    ChannelSend = 24,
    /// Receive from channel
    ChannelRecv = 25,
    /// Map memory
    MapMemory = 30,
    /// Unmap memory
    UnmapMemory = 31,
    /// Allocate memory
    AllocateMemory = 32,
    /// Free memory
    FreeMemory = 33,
    /// Get timer value
    TimerGet = 40,
    /// Set timer
    TimerSet = 41,
    /// Cancel timer
    TimerCancel = 42,
    /// Write log message
    LogWrite = 50,
    // M3 additions.
    /// Create a new IPC namespace.
    NsCreate = 60,
    /// Join an existing IPC namespace.
    NsJoin = 61,
    /// Cross-namespace publish.
    NsPublish = 62,
    /// Cross-namespace subscribe.
    NsSubscribe = 63,
    /// Create a namespace-scoped endpoint.
    NsEndpoint = 64,
    /// Register a process / module manifest for audit.
    ModuleRegister = 70,
    /// Boot a previously-loaded module (start its `entry`).
    ModuleStart = 71,
    /// Load a `.capp` / `.elf` module.
    ModuleLoad = 72,
    /// Cryptographic operations (AES, SHA, TRNG).
    CryptoOp = 80,
    /// Read from flash.
    FlashRead = 81,
    /// Write / program flash.
    FlashWrite = 82,
    /// Erase a flash sector.
    FlashErase = 83,
}

impl Syscall {
    /// Get syscall number
    pub fn number(&self) -> u32 {
        *self as u32
    }

    /// Get syscall from number
    pub fn from_number(num: u32) -> Self {
        match num {
            1 => Self::TaskSpawn,
            2 => Self::TaskExit,
            3 => Self::TaskYield,
            4 => Self::TaskGetId,
            5 => Self::TaskSleep,
            10 => Self::CapDerive,
            11 => Self::CapDuplicate,
            12 => Self::CapRevoke,
            13 => Self::CapFind,
            14 => Self::CapGrant,
            20 => Self::IpcSend,
            21 => Self::IpcRecv,
            22 => Self::IpcReply,
            23 => Self::ChannelCreate,
            24 => Self::ChannelSend,
            25 => Self::ChannelRecv,
            30 => Self::MapMemory,
            31 => Self::UnmapMemory,
            32 => Self::AllocateMemory,
            33 => Self::FreeMemory,
            40 => Self::TimerGet,
            41 => Self::TimerSet,
            42 => Self::TimerCancel,
            50 => Self::LogWrite,
            60 => Self::NsCreate,
            61 => Self::NsJoin,
            62 => Self::NsPublish,
            63 => Self::NsSubscribe,
            64 => Self::NsEndpoint,
            70 => Self::ModuleRegister,
            71 => Self::ModuleStart,
            72 => Self::ModuleLoad,
            80 => Self::CryptoOp,
            81 => Self::FlashRead,
            82 => Self::FlashWrite,
            83 => Self::FlashErase,
            _ => Self::None,
        }
    }
}

/// Syscall arguments
#[derive(Debug, Clone, Copy)]
pub struct SyscallArgs {
    /// Syscall number
    pub number: u32,
    /// First argument
    pub arg0: u32,
    /// Second argument
    pub arg1: u32,
    /// Third argument
    pub arg2: u32,
    /// Fourth argument
    pub arg3: u32,
}

impl SyscallArgs {
    /// Create new syscall arguments
    pub const fn new(number: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> Self {
        Self {
            number,
            arg0,
            arg1,
            arg2,
            arg3,
        }
    }
}

/// Wide-form syscall arguments used when a 64-bit pointer needs to
/// be passed (e.g. on 64-bit hosts). For 32-bit targets the high 32
/// bits are unused.
#[derive(Debug, Clone, Copy)]
pub struct WideSyscallArgs {
    /// Syscall number
    pub number: u32,
    /// First argument
    pub arg0: u32,
    /// Second argument (low 32 bits)
    pub arg1_lo: u32,
    /// Second argument (high 32 bits)
    pub arg1_hi: u32,
    /// Third argument (low 32 bits)
    pub arg2_lo: u32,
    /// Third argument (high 32 bits)
    pub arg2_hi: u32,
    /// Fourth argument
    pub arg3: u32,
}

impl WideSyscallArgs {
    /// Create wide args.
    pub const fn new(
        number: u32,
        arg0: u32,
        arg1: u64,
        arg2: u64,
        arg3: u32,
    ) -> Self {
        Self {
            number,
            arg0,
            arg1_lo: arg1 as u32,
            arg1_hi: (arg1 >> 32) as u32,
            arg2_lo: arg2 as u32,
            arg2_hi: (arg2 >> 32) as u32,
            arg3,
        }
    }

    /// Pointer in `arg1`.
    pub fn arg1_ptr<T>(&self) -> *const T {
        let p = ((self.arg1_hi as u64) << 32) | (self.arg1_lo as u64);
        p as *const T
    }
    /// Pointer in `arg2`.
    pub fn arg2_ptr<T>(&self) -> *mut T {
        let p = ((self.arg2_hi as u64) << 32) | (self.arg2_lo as u64);
        p as *mut T
    }
}

/// Syscall return value
#[derive(Debug, Clone, Copy)]
pub struct SyscallResult {
    /// Return value
    pub value: u32,
    /// Whether the syscall succeeded
    pub success: bool,
    /// Error code (if failed)
    pub error: u32,
}

impl SyscallResult {
    /// Create a successful result
    pub const fn ok(value: u32) -> Self {
        Self {
            value,
            success: true,
            error: 0,
        }
    }

    /// Create an error result
    pub const fn err(error: u32) -> Self {
        Self {
            value: 0,
            success: false,
            error,
        }
    }
}
