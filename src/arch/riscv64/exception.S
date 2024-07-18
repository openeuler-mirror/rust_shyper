.equ RA, 8
.equ SP, 16
.equ GP, 24
.equ TP, 32
.equ T0, 40
.equ T1, 48
.equ T2, 56
.equ S0, 64
.equ S1, 72
.equ A0, 80
.equ A1, 88
.equ A2, 96
.equ A3, 104
.equ A4, 112
.equ A5, 120
.equ A6, 128
.equ A7, 136
.equ S2, 144
.equ S3, 152
.equ S4, 160
.equ S5, 168
.equ S6, 176
.equ S7, 184
.equ S8, 192
.equ S9, 200
.equ S10, 208
.equ S11, 216
.equ T3, 224
.equ T4, 232
.equ T5, 240
.equ T6, 248
.equ SEPC, 256
.equ SCAUSE, 264
.equ STVAL, 272
.equ SSTATUS, 280
.equ SSCRATCH, 288
.equ HART_INFO, 296
.equ HYPER_SP, 304

.equ FRAME_SIZE, (HYPER_SP + 8)


// Save all registers except sp and privileged registers to the data structure formed by sp
.macro SAVE_REGS
    sd ra, RA(sp)
    sd gp, GP(sp)
    sd tp, TP(sp)
    sd t0, T0(sp)
    sd t1, T1(sp)
    sd t2, T2(sp)
    sd s0, S0(sp)
    sd s1, S1(sp)
    sd a0, A0(sp)
    sd a1, A1(sp)
    sd a2, A2(sp)
    sd a3, A3(sp)
    sd a4, A4(sp)
    sd a5, A5(sp)
    sd a6, A6(sp)
    sd a7, A7(sp)
    sd s2, S2(sp)
    sd s3, S3(sp)
    sd s4, S4(sp)
    sd s5, S5(sp)
    sd s6, S6(sp)
    sd s7, S7(sp)
    sd s8, S8(sp)
    sd s9, S9(sp)
    sd s10, S10(sp)
    sd s11, S11(sp)
    sd t3, T3(sp)
    sd t4, T4(sp)
    sd t5, T5(sp)
    sd t6, T6(sp)
.endm

// Recover all registers except sp and privileged registers from the data structure formed by sp
.macro RESTORE_REGS
    ld ra, RA(sp)
    ld gp, GP(sp)
    ld tp, TP(sp)
    ld t0, T0(sp)
    ld t1, T1(sp)
    ld t2, T2(sp)
    ld s0, S0(sp)
    ld s1, S1(sp)
    ld a0, A0(sp)
    ld a1, A1(sp)
    ld a2, A2(sp)
    ld a3, A3(sp)
    ld a4, A4(sp)
    ld a5, A5(sp)
    ld a6, A6(sp)
    ld a7, A7(sp)
    ld s2, S2(sp)
    ld s3, S3(sp)
    ld s4, S4(sp)
    ld s5, S5(sp)
    ld s6, S6(sp)
    ld s7, S7(sp)
    ld s8, S8(sp)
    ld s9, S9(sp)
    ld s10, S10(sp)
    ld s11, S11(sp)
    ld t3, T3(sp)
    ld t4, T4(sp)
    ld t5, T5(sp)
    ld t6, T6(sp)
.endm

.macro VECTOR handler
    // Like FarmOS（xv6），save hypervisor trapframe's address in sscratch reg
    // including hypervisor_sp, tp(Pointer to the cpu information structure corresponding to this cpu) and so on

    // swap sscratch and sp
    csrrw sp, sscratch, sp
    bne sp, zero, virt_entry

// from HS-mode
hs_mode_entry:
    csrrw sp, sscratch, sp
    // sp: hypervisor's sp, sscratch: 0
    addi sp, sp, -FRAME_SIZE

    SAVE_REGS

    csrr s0, sepc
    csrr s1, scause
    csrr s2, stval
    csrr s3, sstatus
    sd s0, SEPC(sp)
    sd s1, SCAUSE(sp)
    sd s2, STVAL(sp)
    sd s3, SSTATUS(sp)

    csrr s0, sscratch

    // pass ctx
    mv a0, sp

    call \handler
    j context_pop

virt_entry:
    // store general regs
    SAVE_REGS

    // save VM's sp to Trapframe
    csrr t0, sscratch
    sd t0, SP(sp)

    // clear sscratch，indicating that we're trapped into kernel mode
    csrw sscratch, zero

    // save Trapframe(sscratch)
    sd sp, SSCRATCH(sp)

    csrr s0, sepc
    csrr s1, scause
    csrr s2, stval
    csrr s3, sstatus
    sd s0, SEPC(sp)
    sd s1, SCAUSE(sp)
    sd s2, STVAL(sp)
    sd s3, SSTATUS(sp)

    # Load the tp saved on the stack
    ld tp, HART_INFO(sp)

    // pass ctx
    mv a0, sp
    mv s0, sp

    ld sp, HYPER_SP(sp) 

    call \handler
    j context_pop
.endm

.global context_vm_entry
context_vm_entry:
    // TODO: Before entry，we need to write sstatus,sscratch,sepc into ctx pointed by a0
    mv sp, a0
    j return_to_vm

context_pop:
    // after 5
    bne s0, zero, return_to_vm_pre

return_to_hypervisor:
    ld s1, SEPC(sp)
    csrw sepc, s1 // set jumping destination

    RESTORE_REGS

    addi sp, sp, FRAME_SIZE
    sret


return_to_vm_pre:
    mv sp, s0 // s0 = old sscratch

return_to_vm:
    ld s1, SEPC(sp)
    ld s2, SSTATUS(sp)
    ld s3, SSCRATCH(sp)
    csrw sepc, s1 // set jumping destination
    csrw sstatus, s2 // Set sstatus，configure the next phase to jump to the S-mode
    csrw sscratch, s3

    // restore general regs
    RESTORE_REGS

    ld sp, SP(sp)
    // Currently, sp is vm's sp，pointing to vm's address space
    sret

.global exception_entry
exception_entry:
    VECTOR exception_rust_handler

