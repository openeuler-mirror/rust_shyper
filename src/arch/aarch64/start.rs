use tock_registers::interfaces::{Writeable, ReadWriteable};

use core::arch::asm;
use crate::arch::PAGE_SIZE;
use crate::kernel::{cpu_map_self, CPU_STACK_OFFSET, CPU_STACK_SIZE};
use crate::board::{PlatOperation, Platform};

// XXX: Fixed boot stack size limits the maximum number of cpus, see '_start'.
const MAX_CPU: usize = 8;

#[repr(align(8), C)]
pub struct BootStack([u8; PAGE_SIZE * 2 * MAX_CPU]);

pub static mut BOOT_STACK: BootStack = BootStack([0; PAGE_SIZE * 2 * MAX_CPU]);

extern "C" {
    fn _bss_begin();
    fn _bss_end();
    fn vectors();
}

#[cfg(any(feature = "tx2"))]
macro_rules! test_cpuid {
    () => {
        r#"
        add x1, x0, 0x1
        cbnz x1, 2f
    1:  wfe
        b   1b

    2:  /*
        * only cluster 1 cpu 0,1,2,3 reach here
        * x0 holds core_id (indexed from zero)
        */"#
    };
}

#[cfg(not(feature = "tx2"))]
macro_rules! test_cpuid {
    () => {
        r#"
        "#
    };
}

#[naked]
#[no_mangle]
#[link_section = ".text.boot"]
pub unsafe extern "C" fn _start() -> ! {
    asm!(
        r#"
        // save fdt pointer to x20
        mov x20, x0 

        // get mpidr2cpu_id
        mrs x0, mpidr_el1
        bl {mpidr2cpuid}
        "#,
        test_cpuid!(),
        r#"
        mov x19, x0 

        // disable cache and MMU
        mrs x1, sctlr_el2
        bic x1, x1, #0xf
        msr sctlr_el2, x1

        // cache_invalidate(0): clear dl1$
        mov x0, #0
        bl  {cache_invalidate}

        // if (cpu_id == 0) cache_invalidate(2): clear l2$
        cbnz x19, 3f
        mov x0, #2
        bl  {cache_invalidate}

    3:  
        // clear icache
        mov x0, x19 
        ic  iallu 

        // setup stack sp per core
        ldr x1, ={boot_stack}
        mov x2, (4096 * 2)
        mul x3, x0, x2
        add x1, x1, x2
        add sp, x1, x3        
        
        // if core_id is not zero, skip bss clearing and pt_populate
        cbnz x0, 4f
        bl {clear_bss}
        adrp x0, {lvl1_page_table}
        adrp x1, {lvl2_page_table}
        bl  {pt_populate}
    4: 
        // Trap nothing from EL1 to El2
        mov x3, xzr
        msr cptr_el2, x3

        // init mmu
        adrp x0, {lvl1_page_table}
        bl  {mmu_init}

        // map cpu page table 
        mrs x0, mpidr_el1
        bl  {cpu_map_self}
        msr ttbr0_el2, x0
        
        // set real sp pointer
        mov x1, 1
        msr spsel, x1
        ldr x1, ={CPU}
        add x1, x1, #({CPU_STACK_OFFSET} + {CPU_STACK_SIZE})
        sub	sp, x1, #{CONTEXT_SIZE}

        bl {init_sysregs}

        tlbi	alle2
        dsb	nsh
        isb

        mov x0, x19
        mov x1, x20
        bl  {init}
        "#,
        mpidr2cpuid = sym Platform::mpidr2cpuid,
        cache_invalidate = sym cache_invalidate,
        boot_stack = sym BOOT_STACK,
        lvl1_page_table = sym super::mmu::LVL1_PAGE_TABLE,
        lvl2_page_table = sym super::mmu::LVL2_PAGE_TABLE,
        pt_populate = sym super::mmu::pt_populate,
        mmu_init = sym super::mmu::mmu_init,
        cpu_map_self = sym cpu_map_self,
        CPU = sym crate::kernel::CPU,
        CPU_STACK_OFFSET = const CPU_STACK_OFFSET,
        CPU_STACK_SIZE = const CPU_STACK_SIZE,
        CONTEXT_SIZE = const core::mem::size_of::<crate::arch::ContextFrame>(),
        clear_bss = sym clear_bss,
        init_sysregs = sym init_sysregs,
        init = sym crate::init,
        options(noreturn)
    );
}

#[naked]
#[no_mangle]
pub unsafe extern "C" fn _secondary_start() -> ! {
    asm!(
        r#"
        // save sp to x20
        mov x20, x0 

        // disable cache and MMU
        mrs x1, sctlr_el2
        bic x1, x1, #0xf
        msr sctlr_el2, x1

        // cache_invalidate(0): clear dl1$
        mov x0, #0
        bl  {cache_invalidate}

        mrs x0, mpidr_el1
        ic  iallu    
        
        mov sp, x20

        // Trap nothing from EL1 to El2
        mov x3, xzr
        msr cptr_el2, x3

        // init mmu
        adrp x0, {lvl1_page_table}
        bl  {mmu_init}

        // map cpu page table 
        mrs x0, mpidr_el1
        bl  {cpu_map_self}
        msr ttbr0_el2, x0
        
        // set real sp pointer
        mov x1, 1
        msr spsel, x1
        ldr x1, ={CPU}
        add x1, x1, #({CPU_STACK_OFFSET} + {CPU_STACK_SIZE})
        sub	sp, x1, #{CONTEXT_SIZE}

        bl {init_sysregs}

        tlbi	alle2
        dsb	nsh
        isb

        mrs x0, mpidr_el1
        bl  {secondary_init}
        "#,
        cache_invalidate = sym cache_invalidate,
        lvl1_page_table = sym super::mmu::LVL1_PAGE_TABLE,
        mmu_init = sym super::mmu::mmu_init,
        cpu_map_self = sym cpu_map_self,
        CPU = sym crate::kernel::CPU,
        CPU_STACK_OFFSET = const CPU_STACK_OFFSET,
        CPU_STACK_SIZE = const CPU_STACK_SIZE,
        CONTEXT_SIZE = const core::mem::size_of::<crate::arch::ContextFrame>(),
        init_sysregs = sym init_sysregs,
        secondary_init = sym crate::secondary_init,
        options(noreturn)
    );
}

fn init_sysregs() {
    use cortex_a::registers::{HCR_EL2, VBAR_EL2, SCTLR_EL2};
    HCR_EL2.set(
        (HCR_EL2::VM::Enable
            + HCR_EL2::RW::EL1IsAarch64
            + HCR_EL2::IMO::EnableVirtualIRQ
            + HCR_EL2::FMO::EnableVirtualFIQ)
            .value
            | 1 << 19, /* TSC */
    );
    VBAR_EL2.set(vectors as usize as u64);
    SCTLR_EL2.modify(SCTLR_EL2::M::Enable + SCTLR_EL2::C::Cacheable + SCTLR_EL2::I::Cacheable);
}

unsafe extern "C" fn clear_bss() {
    core::slice::from_raw_parts_mut(_bss_begin as usize as *mut u8, _bss_end as usize - _bss_begin as usize).fill(0)
}

unsafe extern "C" fn cache_invalidate(cache_level: usize) {
    asm!(
        r#"
        msr csselr_el1, {0}
        mrs x4, ccsidr_el1 // read cache size id.
        and x1, x4, #0x7
        add x1, x1, #0x4 // x1 = cache line size.
        ldr x3, =0x7fff
        and x2, x3, x4, lsr #13 // x2 = cache set number - 1.
        ldr x3, =0x3ff
        and x3, x3, x4, lsr #3 // x3 = cache associativity number - 1.
        clz w4, w3 // x4 = way position in the cisw instruction.
        mov x5, #0 // x5 = way counter way_loop.
    // way_loop:
    1:
        mov x6, #0 // x6 = set counter set_loop.
    // set_loop:
    2:
        lsl x7, x5, x4
        orr x7, {0}, x7 // set way.
        lsl x8, x6, x1
        orr x7, x7, x8 // set set.
        dc cisw, x7 // clean and invalidate cache line.
        add x6, x6, #1 // increment set counter.
        cmp x6, x2 // last set reached yet?
        ble 2b // if not, iterate set_loop,
        add x5, x5, #1 // else, next way.
        cmp x5, x3 // last way reached yet?
        ble 1b // if not, iterate way_loop
        "#,
        in(reg) cache_level,
        options(nostack)
    );
}

#[no_mangle]
unsafe extern "C" fn update_request() {
    asm!(
        r#"
        sub sp, sp, #40
        stp x0, x1, [sp, #0]
        stp x2, x3, [sp, #16]
        str x30, [sp, #32]
    
        adr x2, .   // read pc to x2
        mov x3, #0x8a000000
        cmp x2, x3
        bgt 1f
        bl  {live_update}
    1:
        ldr x30, [sp, #32]
        ldp x2, x3, [sp, #16]
        ldp x0, x1, [sp, #0]
        add sp, sp, #40
        ret
        "#,
        live_update = sym live_update,
    )
}

unsafe extern "C" fn live_update() {
    asm!(
        r#"
        sub sp, sp, #40
        stp x0, x1, [sp, #0]
        stp x2, x3, [sp, #16]
        str x30, [sp, #32]
    
        adr x2, .   // read pc to x0
        sub x2, x2, #16 // sub str instruction
        mov x3, #0x7000000  // 0x83000000 + 0x7000000
        add x2, x2, x3
        blr  x2
    
        ldr x30, [sp, #32]
        ldp x2, x3, [sp, #16]
        ldp x0, x1, [sp, #0]
        add sp, sp, #40
        ret
        "#,
    )
}
