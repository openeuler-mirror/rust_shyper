// TODO: need to support multi update
.global update_request
update_request:
    sub sp, sp, #40
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    str x30, [sp, #32]

    adr x2, .   // read pc to x2
    mov x3, #0x8a000000
    cmp x2, x3
    blt 1f
    bl live_update
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

    bl  rust_shyper_update

    ldr x30, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp, #0]
    add sp, sp, #40
    ret
