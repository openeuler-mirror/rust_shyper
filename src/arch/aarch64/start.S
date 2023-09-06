/*
 * snipet from "Application Note Bare-metal Boot Code for ARMv8-A
 * Processors - Version 1.0"
 *
 * x0 - cache level to be invalidated (0 - dl1$, 1 - il1$, 2 - l2$)
 */
cache_invalidate:
	msr csselr_el1, x0
	mrs x4, ccsidr_el1 // read cache size id.
	and x1, x4, #0x7
	add x1, x1, #0x4 // x1 = cache line size.
	ldr x3, =0x7fff
	and x2, x3, x4, lsr #13 // x2 = cache set number – 1.
	ldr x3, =0x3ff
	and x3, x3, x4, lsr #3 // x3 = cache associativity number – 1.
	clz w4, w3 // x4 = way position in the cisw instruction.
	mov x5, #0 // x5 = way counter way_loop.
way_loop:
	mov x6, #0 // x6 = set counter set_loop.
set_loop:
	lsl x7, x5, x4
	orr x7, x0, x7 // set way.
	lsl x8, x6, x1
	orr x7, x7, x8 // set set.
	dc cisw, x7 // clean and invalidate cache line.
	add x6, x6, #1 // increment set counter.
	cmp x6, x2 // last set reached yet?
	ble set_loop // if not, iterate set_loop,
	add x5, x5, #1 // else, next way.
	cmp x5, x3 // last way reached yet?
	ble way_loop // if not, iterate way_loop
	ret

.global update_request
update_request:
    sub sp, sp, #40
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    str x30, [sp, #32]

    adr x2, .   // read pc to x2
    mov x3, #0x8a000000
    cmp x2, x3
    bgt 1f
    bl  live_update
1:
    ldr x30, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp, #0]
    add sp, sp, #40
    ret

live_update:
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