use core::arch::global_asm;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, AtomicU32};
use super::interface;
use riscv::asm::fence;
use core::arch::asm;
use super::interface::*;
use crate::arch::current_cpu_arch;
use crate::init;
use riscv::register::hideleg::{VSEIP, VSSIP, VSTIP};
use riscv::register::{hcounteren, hedeleg};
use crate::kernel::{cpu_map_self, CpuState, CPU_IF_LIST, CPU_STACK_OFFSET, CPU_STACK_SIZE};
use core::sync::atomic::Ordering::SeqCst;
use crate::secondary_init;
use crate::arch::regs::{SSTATUS_FS, SSTATUS_VS};

const MAX_CPU: usize = NUM_CORE;

static BOOTED_CORES: AtomicU32 = AtomicU32::new(0);
pub static ALLOW_BOOT: AtomicBool = AtomicBool::new(false);
const BOOT_STACK_SIZE: usize = PAGE_SIZE * 2;

// Get the address of the trapframe for each physical core, usually stored in sscratch
pub fn get_trapframe_for_hart(cpu_id: usize) -> u64 {
    // SAFETY:
    // 1. BOOT_STACK is a valid static variable
    // 2. BOOT_STACK_SIZE is a valid constant
    // 3. cpu_id is a valid usize, and it is less than MAX_CPU
    // 4. So, the addr is a valid address which not exceed the BOOT_STACK's range
    unsafe {
        let addr = BOOT_STACK.0.as_ptr().add(cpu_id * BOOT_STACK_SIZE);
        addr as u64
    }
}

// every cpu uses a 2-page stack
#[repr(align(8), C)]
pub struct BootStack([u8; BOOT_STACK_SIZE * MAX_CPU]);

// Put BOOT_STACK in the .bss.stack section，in case it is cleared by clear_bss
#[link_section = ".bss.stack"]
pub static mut BOOT_STACK: BootStack = BootStack([0; BOOT_STACK_SIZE * MAX_CPU]);

// Note: According to the code in section aarch64, the BOOT_STACK is used as the stack only during the startup phase,
// and the given larger Hypervisor stack will be used thereafter

global_asm!(include_str!("exception.S"));

extern "C" {
    fn _bss_begin();
    fn _bss_end();

    pub fn exception_entry();
}

#[naked]
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn _start() -> ! {
    asm!(
        r#"
        // Mask all interrupts
        csrw sie, zero
        
        // Enable FPU
        li t0, {SSTATUS}
        csrs sstatus, t0

        // setup stack sp per core
        la t0, {boot_stack}
        li t1, (4096 * 2)
        mul t2, a0, t1
        add t0, t0, t2
        add sp, t0, t1

        // set core_id to tp
        mv s3, a0
        mv s2, a1

        jal {clear_bss}

        // pass cpu_id(a0) and dtb(a1) to next func
        mv a0, s3
        mv a1, s2
        j {boot_core_init}
        "#,
        SSTATUS = const (SSTATUS_FS | SSTATUS_VS),
        boot_stack = sym BOOT_STACK,
        clear_bss = sym clear_bss,
        boot_core_init = sym boot_core_init,
        options(noreturn)
    );
}

fn init_sysregs() {
    // 3. delegate exceptions and interrupts
    hedeleg::write(
        hedeleg::INST_ADDR_MISALIGN
            | hedeleg::BREAKPOINT
            | hedeleg::ENV_CALL_FROM_U_MODE_OR_VU_MODE
            | hedeleg::INST_PAGE_FAULT
            | hedeleg::LOAD_PAGE_FAULT
            | hedeleg::STORE_AMO_PAGE_FAULT,
    );

    // SAFETY: Set the cycle, time, instret registers to be accessible to VS
    // TODO: These registers can be emulated later to virtualize the clock in the Hypervisor
    unsafe {
        hcounteren::set_cycle();
        hcounteren::set_time();
        hcounteren::set_instret();
    }

    // Note：You need to delegate a virtual Timer Interrupt, otherwise the Hypervisor cannot inject a clock interrupt
    riscv::register::hideleg::write(VSEIP | VSSIP | VSTIP);

    // 4. clear hvip for emulated intr
    riscv::register::hvip::write(0);

    // disable paging
    riscv::register::satp::write(0);

    // 5. set stvec handler
    // SAFETY: exception_entry is a valid assembly function for handling exceptions
    unsafe {
        riscv::register::stvec::write(exception_entry as usize, riscv::register::stvec::TrapMode::Direct);
    }
}

#[no_mangle]
fn boot_core_init(cpu_id: usize, dtb: &mut fdt::myctypes::c_void) {
    riscv_init(true, cpu_id, dtb);
}

#[no_mangle]
fn secondary_core_init(cpu_id: usize, dtb: &mut fdt::myctypes::c_void) {
    riscv_init(false, cpu_id, dtb);
}

// Do some common initialization
#[no_mangle]
pub fn riscv_init(is_init_core: bool, cpu_id: usize, dtb: &mut fdt::myctypes::c_void) {
    init_sysregs();

    if is_init_core {
        println!("boot from core {}", cpu_id);
    }

    // SAFETY:
    // clear .bss segment
    // clear_bss It may affect the value of some local variables, such as the stack allocated for each core.
    // So you need to call rust function after clear_bss
    unsafe { fence() };

    // mark booted cores
    BOOTED_CORES.fetch_or(1 << cpu_id, SeqCst);
    cpu_map_self(cpu_id);

    // The value obtained for current_cpu_arch is valid only after cpu_map_self
    let new_stack = current_cpu_arch() + (CPU_STACK_OFFSET + CPU_STACK_SIZE) as u64
        - core::mem::size_of::<crate::arch::ContextFrame>() as u64;

    // SAFETY:
    // 1. BOOT_STACK is a valid memory space for each core, which is used to save the context of each core
    // 2. Every cpu's stack is 2 pages, so the size of the stack is 4096 * 2, greater than 304+4
    // 3. cpu_id is a valid usize, and it is less than MAX_CPU
    unsafe {
        let addr = BOOT_STACK.0.as_ptr().add(cpu_id * BOOT_STACK_SIZE);
        // Later, we will write scratch to a predefined storage location (i.e. the unused BootStack),
        // Enables the context to be stored directly here when an interruption occurs
        riscv::register::sscratch::write(0);

        // Trapframe saves data in the form of the Riscv64ContextFrame struct in context_frame.rs
        // However, there are some other data after Riscv64ContextFrame, including:
        // tp: Core number, constant (8byte)
        // hypervisor_sp: pointer to the hypervisor stack (8byte)
        // because the hypervisor shares a stack pointer register with the kernel and user
        write_volatile(addr.add(296) as *mut u64, current_cpu_arch());
        write_volatile(addr.add(304) as *mut u64, new_stack);
    }

    // SAFETY:
    // sync BOOT_CORE variable to all cores
    unsafe { fence() };

    if is_init_core {
        arch_boot_secondary_cores(dtb);
    }

    // Wait for other harts to start
    let target_boot_map: u32 = (1 << NUM_CORE) - 1;
    loop {
        // SAFETY:
        // Sync the value of BOOTED_CORES, and if it is equal to target_boot_map,
        // it means that all cores have been booted
        unsafe { fence() };
        if BOOTED_CORES.load(SeqCst) == target_boot_map {
            break;
        }
    }

    if cpu_id == 0 {
        // Setup the kernel stack pointer, and jump to init function
        // SAFETY:
        // 1. tp points to Cpu struct, containing a large stack at offset CPU_STACK_OFFSET
        // 2. dtb_addr is a valid pointer to the device tree blob
        unsafe {
            asm!(
                r#"
                // set real sp pointer(not boot stack, but real large stack)
                mv t0, tp
                add t0, t0, {STACK_TOP}
                sub sp, t0, {CONTEXT_SIZE}
    
                mv a0, {DTB_ADDR}
                jal {init}
                "#,
                STACK_TOP = in(reg) (CPU_STACK_OFFSET + CPU_STACK_SIZE),
                CONTEXT_SIZE = in(reg) core::mem::size_of::<crate::arch::ContextFrame>(),
                DTB_ADDR = in(reg) dtb,
                init = sym init,
                options(noreturn)
            );
        }
    } else {
        loop {
            // SAFETY: Wait for the activation signal from core 0
            unsafe { fence() };
            if ALLOW_BOOT.load(SeqCst) {
                break;
            }
        }

        // SAFETY:
        // tp points to Cpu struct, containing a large stack at offset CPU_STACK_OFFSET
        unsafe {
            asm!(
                r#"
                // set real sp pointer(not boot stack, but real large stack)
                mv t0, tp
                add t0, t0, {STACK_TOP}
                sub sp, t0, {CONTEXT_SIZE}

                mv a0, {CPU_ID}
                jal {secondary_init}
                "#,
                STACK_TOP = in(reg) (CPU_STACK_OFFSET + CPU_STACK_SIZE),
                CONTEXT_SIZE = in(reg) core::mem::size_of::<crate::arch::ContextFrame>(),
                CPU_ID = in(reg) cpu_id,
                secondary_init = sym secondary_init,
                options(noreturn)
            );
        }
    }
}

pub fn arch_boot_secondary_cores(dtb: &mut fdt::myctypes::c_void) {
    // boot other cores
    for i in 0..interface::NUM_CORE {
        if (BOOTED_CORES.load(SeqCst) & (1 << i)) != 0 {
            continue;
        }
        let result = sbi::hsm::hart_start(i, _secondary_start as usize, dtb as *mut _ as usize);
        if let Err(err) = result {
            println!("An error happens when booting core {}: {:?}", i, err);
        }
    }
}

// Send a start signal to non-zero hart
pub fn arch_boot_other_cores() {
    ALLOW_BOOT.store(true, SeqCst);

    // SAFETY: Sync the value of ALLOW_BOOT before running the following code
    unsafe { fence() };

    // Set the state of the other harts to Idle
    let mut cpu_if_list = CPU_IF_LIST.lock();
    for cpu_idx in 1..NUM_CORE {
        if let Some(cpu_if) = cpu_if_list.get_mut(cpu_idx) {
            cpu_if.state_for_start = CpuState::CpuIdle;
        }
    }
}

#[naked]
#[no_mangle]
pub unsafe extern "C" fn _secondary_start() -> ! {
    asm!(
        r#"
        // Mask all interrupts
        csrw sie, zero
        
        // Enable FPU
        li t0, {SSTATUS}
        csrs sstatus, t0

        // setup stack sp per core
        la t0, {boot_stack}
        li t1, (4096 * 2)
        mul t2, a0, t1
        add t0, t0, t2
        add sp, t0, t1
        
        // set core_id to tp
        mv s3, a0
        mv s2, a1

        // pass cpu_id(a0) to next func
        mv a0, s3
        mv a1, s2
        j {secondary_core_init}
        "#,
        SSTATUS = const (SSTATUS_FS | SSTATUS_VS),
        boot_stack = sym BOOT_STACK,
        secondary_core_init = sym secondary_core_init,
        options(noreturn)
    );
}

unsafe extern "C" fn clear_bss() {
    println!(
        "bss_begin: {:016x}, bss_end: {:016x}",
        _bss_begin as usize, _bss_end as usize
    );
    core::slice::from_raw_parts_mut(_bss_begin as *mut u8, _bss_end as usize - _bss_begin as usize).fill(0);
}

pub fn boot_core() -> usize {
    0
}

pub fn is_boot_core(cpu_id: usize) -> bool {
    cpu_id == 0
}
