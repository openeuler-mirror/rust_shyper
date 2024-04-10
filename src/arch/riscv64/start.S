.option norvc

// insert this file to boot section
.section .text.boot

.global _start
_start:
    j _start

