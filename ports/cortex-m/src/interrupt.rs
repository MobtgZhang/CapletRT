//! Interrupt handling for Cortex-M

/// Exception frame pushed by hardware
#[derive(Debug)]
#[repr(C)]
pub struct ExceptionFrame {
    /// R0
    pub r0: u32,
    /// R1
    pub r1: u32,
    /// R2
    pub r2: u32,
    /// R3
    pub r3: u32,
    /// R12
    pub r12: u32,
    /// LR
    pub lr: u32,
    /// PC
    pub pc: u32,
    /// xPSR
    pub xpsr: u32,
}

/// Interrupt frame pushed during interrupt
#[derive(Debug)]
#[repr(C)]
pub struct InterruptFrame {
    /// Basic frame
    pub basic: ExceptionFrame,
    /// R4
    pub r4: u32,
    /// R5
    pub r5: u32,
    /// R6
    pub r6: u32,
    /// R7
    pub r7: u32,
    /// R8
    pub r8: u32,
    /// R9
    pub r9: u32,
    /// R10
    pub r10: u32,
    /// R11
    pub r11: u32,
    /// PSP (Process Stack Pointer)
    pub psp: u32,
}

impl InterruptFrame {
    /// Get the stack pointer
    pub fn stack_pointer(&self) -> u32 {
        self.psp
    }

    /// Set the stack pointer
    pub fn set_stack_pointer(&mut self, sp: u32) {
        self.psp = sp;
    }

    /// Get the program counter
    pub fn pc(&self) -> u32 {
        self.basic.pc
    }

    /// Get the link register
    pub fn lr(&self) -> u32 {
        self.basic.lr
    }
}

/// Exception numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exception {
    /// Reset
    Reset = 1,
    /// Non-maskable interrupt
    Nmi = 2,
    /// Hard fault
    HardFault = 3,
    /// Memory management fault
    MemManage = 4,
    /// Bus fault
    BusFault = 5,
    /// Usage fault
    UsageFault = 6,
    /// SVCall
    SvCall = 11,
    /// Debug monitor
    DebugMonitor = 12,
    /// PendSV
    PendSV = 14,
    /// SysTick
    SysTick = 15,
}

impl Exception {
    /// Get the exception number
    pub fn number(&self) -> u8 {
        *self as u8
    }

    /// Get from exception number
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            1 => Some(Exception::Reset),
            2 => Some(Exception::Nmi),
            3 => Some(Exception::HardFault),
            4 => Some(Exception::MemManage),
            5 => Some(Exception::BusFault),
            6 => Some(Exception::UsageFault),
            11 => Some(Exception::SvCall),
            12 => Some(Exception::DebugMonitor),
            14 => Some(Exception::PendSV),
            15 => Some(Exception::SysTick),
            _ => None,
        }
    }
}

/// Interrupt numbers (starts at 16)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interrupt(u8);

impl Interrupt {
    /// Create a new interrupt
    pub fn new(n: u8) -> Self {
        Self(n)
    }

    /// Get the interrupt number
    pub fn number(&self) -> u8 {
        self.0
    }

    /// Get the exception number (for NVIC)
    pub fn exception_number(&self) -> u8 {
        self.0 + 16
    }
}
